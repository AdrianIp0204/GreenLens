import type { RunAssessment } from '../climate-types';

export function qualificationLabel(assessment: RunAssessment | null | undefined): string {
  switch (assessment?.scientific_qualification.status) {
    case 'failed': return 'Numerical acceptance failed';
    case 'limited_qualified': return 'Qualified only within the stated scope';
    default: return 'Scientific qualification unassessed';
  }
}

/** Presentation only. Status, scope and identity matching come from Rust. */
export function QualificationStatus({assessment,error='',label='Result'}:{assessment:RunAssessment|null;error?:string;label?:string}) {
  const q=assessment?.scientific_qualification;
  const status=q?.status==='failed'?'failed':q?.status==='limited_qualified'?'limited':'unassessed';
  const relation=assessment?.current_input_relation;
  return <section className={`qualification-status ${status}`} aria-label={`${label} scientific qualification`}>
    <strong>{label}: {qualificationLabel(assessment)}</strong>
    <p>{q?.status==='failed'&&q.evidence.length?q.evidence.map(e=>e.reason).join(' '):q?.reason??'No applicable assessment is available. Completed execution or matching current inputs does not establish numerical accuracy.'}</p>
    {error&&<p role="alert">Assessment could not be read: {error}</p>}
    {assessment&&q&&<>
      <small>{q.profile_display_name} · {q.identity.method_version}</small>
      {q.evidence.length>0&&<small>Evidence: {q.evidence.map(e=>e.id).join('; ')}</small>}
      <details><summary>Execution, integrity, current inputs and assessment evidence</summary>
        <p>{q.reason}</p>
        <dl>
          <dt>Execution</dt><dd>{assessment.process_completion==='retained_successful_exit_records'?'Retained required process exits succeeded.':'No completion assessment available.'}</dd>
          <dt>Evidence integrity</dt><dd>{assessment.evidence_integrity==='full_retained_evidence_checked'?'Full retained evidence checked.':assessment.evidence_integrity==='sealed_metadata_checked'?'Sealed metadata checked; this assessment did not reread the full time series.':assessment.evidence_integrity==='validated_record_only'?'Record structure validated; retained files were not checked by this assessment.':'No evidence-integrity assessment available.'}</dd>
          <dt>Current physical inputs</dt><dd>{relation?.physical==='matches'?'Match this snapshot.':relation?.physical==='differs'?'Differ from this snapshot.':'Not compared.'}</dd>
          <dt>Saved metric settings</dt><dd>{relation?.metrics==='matches'?'Match current settings.':relation?.metrics==='differs'?'Differ from current settings. Derived display metrics may use current settings.':relation?.metrics==='not_applicable'?'Not applicable to a selected-hour snapshot.':'Not compared.'}</dd>
        </dl>
        {q.evidence.map(e=><div className="qualification-evidence" key={`${e.id}:${e.sha256}`}><b>{e.id}</b><p>{e.reason}</p><small>Tested scope: {e.tested_scope}</small><code>{e.sha256}</code></div>)}
        {q.limitations.map((text,i)=><p key={i}>{text}</p>)}
        <small>Assessment scope: {q.identity.scope}<br/>Registry {q.registry_version}</small>
        <code title="Complete numerical profile SHA-256">Profile {q.identity.profile_sha256}</code>
        <code title="Complete engine identity SHA-256">Engine {q.identity.engine_sha256}</code>
        <code title="Recognized recipe contract SHA-256">Recipe contract {q.identity.recipe_contract_sha256}</code>
        <code title="Immutable run recipe SHA-256">Run recipe {assessment.recipe_sha256}</code>
        <code title="Qualification registry SHA-256">Registry {q.registry_sha256}</code>
      </details>
    </>}
  </section>;
}
