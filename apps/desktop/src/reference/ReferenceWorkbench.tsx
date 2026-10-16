import { useEffect, useMemo, useRef, useState } from 'react';
import type { ArchitecturalScope, AssumptionStatus, MappingStatus, ReferenceImportDraft, ReferenceImportOptions, ReferenceModel, ReferenceProgress, ReferenceScopeDraft, ReferenceWorkbenchDraft, ReconciliationMeasure, ScopeAssessment, ScopeAssumption, ScopeContext, ScopeMapping, ScopeReconciliation, ScopeUnit } from './types';
import { newImportDraft, newReferenceDraft, newScopeDraft, parseReferenceImportDraft, parseReferenceScopeDraft } from './reference-drafts';
import type { ReferenceOccurrenceSelection } from './selection';
import ReferenceOutliner from './ReferenceOutliner';
import './reference-workbench.css';

export type ReferenceWorkbenchProps = {
  documentId: string; reference: ReferenceModel | null; scope: ArchitecturalScope | null;
  assessment: ScopeAssessment | null; context: ScopeContext | null;
  draft: ReferenceWorkbenchDraft | null; onDraftChange: (draft: ReferenceWorkbenchDraft | null) => void;
  onConsumeDraft: (kind: 'import' | 'scope', consumed: string, documentId: string) => boolean;
  inspectedOccurrence: ReferenceOccurrenceSelection | null;
  onInspectOccurrence: (selection: ReferenceOccurrenceSelection | null) => void;
  activeOccurrenceId: string | null;
  onSelectOccurrence: (id: string | null) => void;
  onImport: (options: ReferenceImportOptions) => Promise<boolean>;
  onAssess: (scope: ArchitecturalScope) => Promise<ScopeAssessment>;
  onSaveScope: (scope: ArchitecturalScope) => Promise<boolean>; onClearScope: () => Promise<boolean>;
  importProgress?: ReferenceProgress | null; onCancelImport?: () => void;
  disabled?: boolean; disabledReason?: string;
  candidateMode?: boolean; onFrameSelected?: () => void;
};
const mappingLabels: Record<MappingStatus, string> = { mapped: 'Mapped to analytical inputs', context_only: 'Context only', excluded: 'Excluded from study', unresolved: 'Unresolved' };
const assumptionLabels: Record<AssumptionStatus, string> = { documented: 'Documented', measured: 'Measured', assumed: 'Assumed', synthetic: 'Synthetic', missing: 'Missing', not_applicable: 'Not applicable' };
const measures: Record<ReconciliationMeasure, [string, ScopeUnit, string]> = {
  local_length_x: ['Local length X', 'metres', 'm'], local_length_y: ['Local length Y', 'metres', 'm'], height: ['Height', 'metres', 'm'],
  floor_area: ['Floor area', 'square_metres', 'm²'], volume: ['Volume', 'cubic_metres', 'm³'],
  exterior_envelope_gross_area: ['Gross exterior envelope', 'square_metres', 'm²'], opening_area: ['Opening area', 'square_metres', 'm²'], north_rotation: ['North rotation', 'degrees_clockwise', '° clockwise'],
};
const clone = <T,>(value: T): T => structuredClone(value);
const nextId = (prefix: string, ids: string[]) => { let i = 1; while (ids.includes(`${prefix}-${i}`)) i++; return `${prefix}-${i}`; };
const short = (hash: string) => hash.slice(0, 12);
const mappingPageSize = 100;

/** Form drafts are controlled by the document service so navigation cannot erase
 * them. Every accepted import/scope action is delegated to shared Rust services. */
export default function ReferenceWorkbench(props: ReferenceWorkbenchProps) {
  const { reference, scope, context, assessment, documentId } = props;
  const candidateMode=Boolean(props.candidateMode);
  const [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<ScopeAssessment | null>(null);
  const [mappingQuery, setMappingQuery] = useState(''), [mappingPage, setMappingPage] = useState(0);
  const [confirm, setConfirm] = useState<'discard' | 'clear' | 'restart' | null>(null);
  const confirmation = useRef<HTMLDivElement>(null), cancelButton = useRef<HTMLButtonElement>(null);
  const confirmationTrigger = useRef<{ documentId: string; button: HTMLButtonElement } | null>(null);
  const restoreTrigger = useRef<typeof confirmationTrigger.current>(null);
  const mounted = useRef(true); useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const signature = JSON.stringify([documentId, props.draft, reference?.revision_sha256, context]);
  const latest = useRef(signature); latest.current = signature;
  const previewSignature = useRef('');
  useEffect(() => { setPreview(null); setError(''); }, [signature]);
  useEffect(() => { setMappingQuery(''); setMappingPage(0); setConfirm(null); confirmationTrigger.current = null; restoreTrigger.current = null; }, [documentId]);
  useEffect(() => {
    if (confirm) {
      if (confirmationTrigger.current?.documentId !== documentId) return;
      // This inline prompt precedes a long editor. Move to its safe choice and
      // reveal it even when the action originated at the bottom of the panel.
      cancelButton.current?.focus({ preventScroll: true });
      confirmation.current?.scrollIntoView?.({ block: 'nearest' });
    } else {
      const trigger = restoreTrigger.current;
      restoreTrigger.current = null; confirmationTrigger.current = null;
      if (trigger?.documentId === documentId && trigger.button.isConnected && !trigger.button.disabled) {
        trigger.button.focus({ preventScroll: true });
        trigger.button.scrollIntoView?.({ block: 'nearest' });
      }
    }
  }, [confirm, documentId]);
  const controlled = props.draft?.document_id === documentId ? props.draft : null;
  const edit = (change: (draft: ReferenceWorkbenchDraft) => void) => {
    const candidate = clone(controlled ?? newReferenceDraft(documentId)); change(candidate); props.onDraftChange(candidate);
  };
  const updateScope = (change: (value: ArchitecturalScope, draft: ReferenceScopeDraft) => void) => edit(d => { if (d.scope) change(d.scope.value, d.scope); });
  const candidate = controlled?.scope?.value ?? null;
  const occurrenceById = useMemo(() => new Map(candidate?.reference_revision_sha256 === reference?.revision_sha256 ? reference?.occurrences.map(o => [o.id, o]) : []), [reference, candidate?.reference_revision_sha256]);
  const filteredMappings = useMemo(() => {
    const needle = mappingQuery.trim().toLowerCase();
    return candidate?.mappings.filter(m => [m.occurrence_id, occurrenceById.get(m.occurrence_id)?.label ?? '', m.status, mappingLabels[m.status], ...m.targets.map(t => `${t.design_id}: ${t.object_id}`)].join(' ').toLowerCase().includes(needle)) ?? [];
  }, [candidate?.mappings, mappingQuery, occurrenceById]);
  const shownMappingPage = Math.min(mappingPage, Math.max(0, Math.ceil(filteredMappings.length / mappingPageSize) - 1));
  const mappingStart = shownMappingPage * mappingPageSize;
  const visibleMappings = filteredMappings.slice(mappingStart, mappingStart + mappingPageSize);
  const lock = Boolean(props.disabled || busy || props.importProgress);
  const requestConfirmation = (kind: NonNullable<typeof confirm>, button: HTMLButtonElement) => {
    confirmationTrigger.current = { documentId, button }; restoreTrigger.current = null; setConfirm(kind);
  };
  const cancelConfirmation = () => {
    if (lock) return;
    restoreTrigger.current = confirmationTrigger.current; setConfirm(null);
  };
  const selected = props.inspectedOccurrence?.occurrence_id ?? '';
  const inspectionRevision = props.inspectedOccurrence?.revision_sha256;
  const inspectionMatchesCandidate = Boolean(candidate && candidate.reference_revision_sha256 === inspectionRevision);
  const selectedForScope = inspectionMatchesCandidate ? selected : '';
  const occurrence = reference && reference.revision_sha256 === inspectionRevision ? reference.occurrences.find(o => o.id === selected) : undefined;
  const mapping = inspectionMatchesCandidate ? candidate?.mappings.find(m => m.occurrence_id === selected) : undefined;
  const select = (id: string) => { props.onSelectOccurrence(id || null); if (!id) props.onInspectOccurrence(null); };
  const changeMapping = (change: (value: ScopeMapping) => void) => updateScope(value => {
    if (!selected || !inspectionMatchesCandidate) return;
    let row = value.mappings.find(m => m.occurrence_id === selected);
    if (!row) { row = { occurrence_id: selected, status: 'unresolved', targets: [], rationale: '' }; value.mappings.push(row); }
    change(row);
  });
  const perform = async (action: () => Promise<void>) => {
    if (lock) return; setBusy(true); setError('');
    try { await action(); } catch (e) { if (mounted.current) setError(String(e)); }
    finally { if (mounted.current) setBusy(false); }
  };
  const stillCurrent = (before: string) => mounted.current && latest.current === before;
  const clearConsumed = (kind: 'import' | 'scope', consumed: string, owner: string) => {
    // Acceptance itself changes the reference/context. Match the exact consumed
    // category in its durable owner even if navigation unmounted this panel.
    props.onConsumeDraft(kind, consumed, owner);
  };
  const startScope = () => {
    if (!reference || !context) return;
    edit(d => { d.scope = newScopeDraft(reference, context); }); setConfirm(null);
  };
  const review = () => perform(async () => {
    if (!controlled?.scope) return; const before = signature;
    const result = await props.onAssess(parseReferenceScopeDraft(controlled.scope));
    if (stillCurrent(before)) { previewSignature.current = before; setPreview(result); }
  });
  const save = () => perform(async () => {
    if (!controlled?.scope) return; const before = signature, consumed = JSON.stringify(controlled.scope);
    if (!preview || preview.relationship !== 'current' || previewSignature.current !== before) throw new Error('Review the current scope in Rust before saving. The accepted scope is unchanged.');
    if (await props.onSaveScope(parseReferenceScopeDraft(controlled.scope))) {
      clearConsumed('scope', consumed, documentId);
    } else throw new Error('Scope was not accepted. The complete draft is retained; inspect the document error and review again.');
  });
  const importFile = () => perform(async () => {
    if (!controlled?.import) return; const consumed = JSON.stringify(controlled.import);
    if (await props.onImport(parseReferenceImportDraft(controlled.import))) {
      clearConsumed('import', consumed, documentId);
    }
  });
  const input = (label: string, value: string, change: (text: string) => void, numeric = false) => <label>{label}<input value={value} inputMode={numeric ? 'decimal' : undefined} onChange={e => change(e.target.value)} /></label>;
  const area = (label: string, value: string, change: (text: string) => void) => <label>{label}<textarea value={value} onChange={e => change(e.target.value)} /></label>;
  const importField = (key: keyof ReferenceImportDraft, label: string, numeric = false) => input(label, String(controlled?.import?.[key] ?? ''), text => edit(d => { if (d.import) (d.import as unknown as Record<string, unknown>)[key] = text; }), numeric);
  const changeAssumption = (id: string, change: (row: ScopeAssumption) => void) => updateScope(value => { const row = value.assumptions.find(a => a.id === id); if (row) change(row); });
  const changeReconciliation = (id: string, change: (row: ScopeReconciliation, draft: ReferenceScopeDraft) => void) => updateScope((value, draft) => { const row = value.reconciliations.find(r => r.id === id); if (row) change(row, draft); });
  const boundDesigns = candidate ? Object.keys(candidate.design_model_sha256) : [];
  const allTargets = context ? Object.entries(context.designs).flatMap(([design_id, design]) => design.targets.map(t => ({ ...t, design_id, model_input_sha256: design.model_input_sha256 }))) : [];
  // A current object's name must not be presented as the name of a target bound
  // to a different historical model. Stable IDs remain visible in either case.
  const targetLabels = new Map(allTargets.map(t => [JSON.stringify([t.design_id, t.object_id, t.kind, t.model_input_sha256]), t.label]));
  const staleBinding = candidate && (candidate.reference_revision_sha256 !== reference?.revision_sha256 || boundDesigns.some(id => candidate.design_model_sha256[id] !== context?.designs[id]?.model_input_sha256));

  return <section className="reference-workbench" aria-label={candidateMode?'Editable carry-forward candidate':'Architectural reference and study scope'} onKeyDown={event => {
    if (event.key === 'Escape' && !event.nativeEvent.isComposing && !confirm && props.inspectedOccurrence) {
      event.preventDefault(); event.stopPropagation(); props.onInspectOccurrence(null);
    }
  }}>
    {!candidateMode&&<><header><h2>Architectural reference</h2><p>Read-only reference geometry. Thermal inputs, analysis inclusion and scientific qualification remain separate.</p></header>
    {props.disabled && <p role="status">{props.disabledReason ?? 'Resolve the active document operation before changing reference inputs.'}</p>}
    {controlled && (controlled.import || controlled.scope) && <p className="reference-draft-note">Unapplied reference draft — accepted geometry and scope are unchanged.</p>}
    {props.draft && !controlled && <p role="alert">A draft belongs to another document and cannot be applied here.</p>}
    {error && <p role="alert">{error}</p>}
    {props.importProgress && <div role="status"><p>{props.importProgress.stage}: {props.importProgress.completed} / {props.importProgress.total}; {props.importProgress.processed_bytes.toLocaleString()} bytes.</p><progress value={props.importProgress.completed} max={Math.max(1, props.importProgress.total)} />{props.onCancelImport && <button onClick={props.onCancelImport}>Cancel reference import</button>}</div>}

    <details open={!reference || Boolean(controlled?.import)}><summary>Local GLB intake and coordinate choices</summary>
      <p>Export GLB 2.0 locally using a legitimately available application. The original source stays external. No external URLs, scripts or thermal properties are imported.</p>
      {!controlled?.import ? <button disabled={lock} onClick={() => edit(d => { d.import = newImportDraft(); })}>{reference ? 'Prepare another reference revision' : 'Prepare local GLB import'}</button> : <fieldset disabled={lock}>
        <div className="reference-form-grid">{importField('scale', 'Metres per source unit', true)}{importField('rotation', 'Model rotation (° clockwise)', true)}{importField('x', 'Translation X (m)', true)}{importField('y', 'Translation Y (m)', true)}{importField('z', 'Translation Z (m)', true)}{importField('scene', 'Scene index (blank = default)', true)}{importField('north', 'Declared true north (°; blank = unknown)', true)}</div>
        <p>GLB specifies metres and Y-up. Rust converts into right-handed X/Y/Z-up coordinates before applying the explicit scale, rotation and translation. North and location are never inferred.</p>
        <div className="reference-form-grid">{importField('application', 'Local conversion application')}{importField('version', 'Conversion version')}{importField('options', 'Conversion options')}{importField('note', 'Conversion provenance / rights note')}</div>
        <label><input type="checkbox" checked={controlled.import.dimensions_checked} onChange={e => edit(d => { if (d.import) d.import.dimensions_checked = e.target.checked; })} /> I reviewed the explicit scale; source dimension checks must be recorded before relying on a mapping.</label>
        <label><input type="checkbox" checked={controlled.import.orientation_checked} onChange={e => edit(d => { if (d.import) d.import.orientation_checked = e.target.checked; })} /> I reviewed the axis, origin and orientation choices; unknown true north stays unknown.</label>
        {reference && <p>Reimport retains the previous admitted assets and accepted scope. Existing mappings remain bound to their old revision and become historical.</p>}
        <button onClick={() => void importFile()}>Choose local GLB and import</button><button onClick={() => edit(d => { d.import = null; })}>Cancel intake draft</button>
      </fieldset>}
    </details>

    {reference && <details open><summary>Retained import receipt</summary>
      <dl className="reference-facts"><dt>Importer / revision</dt><dd>{reference.importer_id} · <code title={reference.revision_sha256}>{short(reference.revision_sha256)}</code></dd><dt>Geometry</dt><dd>{reference.occurrences.length.toLocaleString()} occurrences · {reference.triangle_count.toLocaleString()} triangles · {reference.chunks.length} sealed chunks</dd><dt>Bounds, X / Y / Z (m)</dt><dd>{reference.bounds.min_m.join(', ')} to {reference.bounds.max_m.join(', ')}</dd><dt>True north</dt><dd>{reference.options.declared_north_clockwise_deg === null ? 'Unknown' : `${reference.options.declared_north_clockwise_deg}° clockwise (declared)`}</dd><dt>Intake resource receipt</dt><dd>{reference.resources.elapsed_ms} ms; {reference.resources.normalized_bytes.toLocaleString()} normalized bytes; {reference.resources.decoded_allocation_estimate_bytes.toLocaleString()} estimated allocation bytes (not measured process memory)</dd><dt>Storage</dt><dd>{reference.storage_policy}. This is not a complete architectural-source archive.</dd></dl>
      <p>{reference.coordinate_convention}. Bounds are not a measured floor area, volume or whole-building denominator.</p>
      {reference.warnings.map((value, i) => <p className="reference-warning" key={`w${i}`}>{value}</p>)}
      <h4>Excluded or unsupported source features</h4>{reference.exclusions.length ? <ul>{reference.exclusions.map((value, i) => <li key={i}>{value}</li>)}</ul> : <p>No additional exclusions declared by this importer.</p>}
    </details>}

    <section aria-label="Accepted scope status"><h3>Study scope</h3>
      {scope ? <><p>Accepted: {scope.title} · {scope.scope_kind.replaceAll('_', ' ')} · reference <code title={scope.reference_revision_sha256}>{short(scope.reference_revision_sha256)}</code>.</p>{assessment ? <Assessment value={assessment} /> : <p>Scope relationship has not been assessed for the current inputs.</p>}</> : <p>No analytical scope is accepted. Visible geometry alone does not establish study coverage.</p>}
      <div className="reference-actions">{scope && !candidate && <button disabled={lock} onClick={() => edit(d => { d.scope = { value: clone(scope), numbers: {} }; })}>Edit accepted scope</button>}<button disabled={lock || !reference || !context} onClick={e => candidate || scope ? requestConfirmation('restart', e.currentTarget) : startScope()}>Start scope for current revision</button>{scope && <button disabled={lock} onClick={e => requestConfirmation('clear', e.currentTarget)}>Clear accepted scope…</button>}</div>
      {!context && reference && <p>Valid accepted thermal designs and the Rust scope context are required to map this reference.</p>}
      {Boolean(scope?.report_views?.length)&&<details><summary>Accepted portable report views ({scope!.report_views!.length})</summary><ul>{scope!.report_views!.map(view=><li key={view.id}>{view.title} · design {view.design_id} · {view.vertical_span_m} m vertical frame · {view.aspect_ratio.toFixed(3)} width/height{view.section?` · section ${view.section.axis}=${view.section.offset_m} m, keep ${view.section.keep}`:' · no section'}. Reference {short(view.reference_revision_sha256)}.</li>)}</ul><p>Definitions travel with the accepted scope and change only report identities. Source assets and analytical inputs remain independent.</p></details>}
    </section>

    </>}
    {candidateMode&&<p>The editable candidate is separate from the immutable parent and Rust proposal. Changes are applied only by accepting the reviewed revision.</p>}
    {confirm && <div ref={confirmation} role="alertdialog" aria-label="Confirm reference scope action" onKeyDown={e => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); cancelConfirmation(); }
    }}><p>{confirm === 'clear' ? 'Clear only the accepted study scope? Retained geometry and simulation evidence remain. Any unapplied scope draft stays available.' : confirm === 'restart' ? 'Start a new draft bound to the current reference and accepted thermal designs? This replaces the current scope draft; the accepted scope remains until a valid save.' : 'Discard the unapplied scope draft and its preview? Accepted inputs remain unchanged.'}</p><button disabled={lock} onClick={() => {
      if (confirm === 'restart') startScope();
      else if (confirm === 'discard') { edit(d => { d.scope = null; }); setConfirm(null); }
      else void perform(async () => { if (await props.onClearScope()) setConfirm(null); });
    }}>{confirm === 'clear' ? 'Clear scope' : confirm === 'restart' ? 'Start new scope draft' : 'Discard scope draft'}</button><button ref={cancelButton} disabled={lock} onClick={cancelConfirmation}>Cancel — keep editing</button></div>}

    {candidate && controlled?.scope && <fieldset disabled={lock || Boolean(confirm)} className="reference-scope-editor">
      <legend>{candidateMode?'Editable candidate scope':'Unapplied scope and source mapping'}</legend>
      {staleBinding && <p role="alert">This draft binds a different reference or accepted model. Review its historical mapping. Start a new scope for current inputs; saving must not silently rebind it.</p>}
      <div className="reference-form-grid">{input('Scope title', candidate.title, text => updateScope(v => { v.title = text; }))}<label>Scope classification<select value={candidate.scope_kind} onChange={e => updateScope(v => { v.scope_kind = e.target.value as ArchitecturalScope['scope_kind']; })}><option value="partial_building">Partial building</option><option value="representative_study">Representative study</option><option value="whole_building">Whole building — requires complete review</option></select></label></div>
      {area('Study purpose', candidate.purpose, text => updateScope(v => { v.purpose = text; }))}
      <div className="reference-form-grid">{area('Included scope (one item per line)', candidate.included_scope.join('\n'), text => updateScope(v => { v.included_scope = text.split('\n'); }))}{area('Excluded scope (one item per line)', candidate.excluded_scope.join('\n'), text => updateScope(v => { v.excluded_scope = text === '' ? [] : text.split('\n'); }))}</div>
      {area('Boundary-condition consequences', candidate.boundary_consequences, text => updateScope(v => { v.boundary_consequences = text; }))}
      <p>Include storeys, spaces, envelope, context and systems. Partial-study energy is never extrapolated to the whole building.</p>
      <details open={Boolean(candidate.report_views?.length)}><summary>Portable report figure draft ({candidate.report_views?.length??0}/8)</summary><p>In an orthographic viewport, enter a view name and choose Add active view to report draft. Review and save the whole scope before exporting. Figures include reference and analytical geometry in the retained frame/section; transient hiding and selection are not copied.</p>{candidate.report_views?.map(view=><fieldset key={view.id} data-scope-key={`report_view:${view.id}`}><legend>{view.id} · {view.design_id}</legend>{input('Report figure title',view.title,title=>updateScope(v=>{const item=v.report_views?.find(item=>item.id===view.id);if(item)item.title=title;}))}<p>Eye: {view.eye_m.join(', ')} m; target: {view.target_m.join(', ')} m. Vertical span {view.vertical_span_m} m, frame ratio {view.aspect_ratio}. {view.section?`Section ${view.section.axis}=${view.section.offset_m} m, keep ${view.section.keep}.`:'No section.'}</p><p>Reference <code>{short(view.reference_revision_sha256)}</code>; analytical model <code>{short(view.model_input_sha256)}</code>.</p><button onClick={()=>updateScope(v=>{v.report_views=v.report_views?.filter(item=>item.id!==view.id);})}>Remove report view {view.id}</button></fieldset>)}</details>
      <details open><summary>Revision-scoped object mappings</summary>
        {reference&&<ReferenceOutliner documentKey={documentId} reference={reference} selectedOccurrenceId={props.activeOccurrenceId} onSelectOccurrence={id=>select(id??'')} onFrameSelected={props.onFrameSelected} disabled={lock}/>}

        {selected && <div className="reference-mapping"><p>Inspecting <code>{selected}</code> · revision <code>{short(inspectionRevision ?? '')}</code>{occurrence?.mirrored ? ' · mirrored occurrence' : ''}{!occurrence ? ' · absent from active revision' : ''}.</p>{!inspectionMatchesCandidate && <p role="status">This occurrence belongs to another reference revision than the scope draft. Choose a retained mapping to inspect that exact row; no mapping is edited by this live selection.</p>}<button onClick={() => props.onInspectOccurrence(null)}>Clear mapping inspection</button><fieldset disabled={!inspectionMatchesCandidate}><legend>Exact revision mapping</legend><label>Mapping state<select value={mapping?.status ?? 'unresolved'} onChange={e => changeMapping(m => { m.status = e.target.value as MappingStatus; if (m.status === 'excluded' || m.status === 'unresolved') m.targets = []; if (m.status === 'context_only') m.targets = m.targets.filter(t => t.kind === 'detached_shading'); })}>{Object.entries(mappingLabels).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>
          {area('Mapping rationale', mapping?.rationale ?? '', text => changeMapping(m => { m.rationale = text; }))}
          {(mapping?.status === 'mapped' || mapping?.status === 'context_only') && <fieldset><legend>Explicit analytical targets</legend>{allTargets.filter(t => boundDesigns.includes(t.design_id) && (mapping.status !== 'context_only' || t.kind === 'detached_shading')).map(t => <label key={`${t.design_id}/${t.object_id}`}><input type="checkbox" checked={mapping.targets.some(x => x.design_id === t.design_id && x.object_id === t.object_id)} onChange={e => changeMapping(m => { m.targets = m.targets.filter(x => x.design_id !== t.design_id || x.object_id !== t.object_id); if (e.target.checked) { const { label: _label, ...target } = t; m.targets.push(target); } })} />{t.design_id} · {t.label} · {t.kind.replaceAll('_', ' ')} · {t.object_id}</label>)}</fieldset>}
          {mapping && <button onClick={() => updateScope(v => { v.mappings = v.mappings.filter(m => m.occurrence_id !== selected); })}>Remove explicit classification</button>}
        </fieldset></div>}
        {input('Search retained mappings', mappingQuery, text => { setMappingQuery(text); setMappingPage(0); })}
        <p aria-live="polite">Showing {filteredMappings.length ? mappingStart + 1 : 0}–{mappingStart + visibleMappings.length} of {filteredMappings.length.toLocaleString()} matching retained mappings ({candidate.mappings.length.toLocaleString()} total). Search includes historical occurrence IDs, labels, states and analytical targets.</p>
        <div className="reference-actions"><button disabled={shownMappingPage === 0} onClick={() => setMappingPage(shownMappingPage - 1)}>Previous mappings</button><button disabled={mappingStart + visibleMappings.length >= filteredMappings.length} onClick={() => setMappingPage(shownMappingPage + 1)}>Next mappings</button></div>
        <div className="reference-table reference-mapping-table"><table><caption>Explicit source classifications — omitted occurrences remain unresolved</caption><colgroup><col className="reference-occurrence-column" /><col className="reference-state-column" /><col className="reference-target-column" /></colgroup><thead><tr><th scope="col">Occurrence</th><th scope="col">State</th><th scope="col">Targets</th></tr></thead><tbody>{visibleMappings.map(m => <tr key={m.occurrence_id}><th scope="row"><button className="reference-occurrence-button" title={m.occurrence_id} aria-pressed={inspectionMatchesCandidate && selected === m.occurrence_id} onClick={() => props.onInspectOccurrence({ revision_sha256: candidate.reference_revision_sha256, occurrence_id: m.occurrence_id })}><span>{occurrenceById.get(m.occurrence_id)?.label || 'Historical occurrence'}</span><code>{m.occurrence_id}</code></button></th><td>{mappingLabels[m.status]}</td><td>{m.targets.length ? <ul className="reference-target-list">{m.targets.map(t => {
          const label = targetLabels.get(JSON.stringify([t.design_id, t.object_id, t.kind, t.model_input_sha256]));
          return <li key={`${t.design_id}/${t.object_id}`}><span>{t.design_id}: {label || t.object_id}</span>{label && <code>{t.object_id}</code>}</li>;
        })}</ul> : 'No analytical target'}</td></tr>)}</tbody></table></div>
      </details>

      <details><summary>Assumption and readiness register ({candidate.assumptions.length})</summary>
        <p>Geometry cannot establish thermal constructions, occupancy, schedules or equipment. Declare sources and affected scope; missing and not applicable remain distinct.</p>
        <button onClick={() => updateScope(v => { v.assumptions.push({ id: nextId('assumption', v.assumptions.map(a => a.id)), link: { kind: 'study' }, field: '', status: 'missing', value: null, units: '', source: '', rationale: '', affected_scope: '' }); })}>Add assumption</button>
        {candidate.assumptions.map(a => <fieldset key={a.id} data-scope-key={`assumption:${a.id}`}><legend>{a.id}</legend><div className="reference-form-grid"><label>Input status<select value={a.status} onChange={e => changeAssumption(a.id, row => { row.status = e.target.value as AssumptionStatus; row.value = row.status === 'missing' || row.status === 'not_applicable' ? null : row.value ?? ''; })}>{Object.entries(assumptionLabels).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label><label>Linked scope<select value={a.link.kind} onChange={e => changeAssumption(a.id, row => { row.link = e.target.value === 'study' ? { kind: 'study' } : e.target.value === 'study_field'||e.target.value === 'calendar_field' ? {kind:e.target.value,design_id:boundDesigns[0]??''} : e.target.value === 'reference_occurrence' ? { kind: 'reference_occurrence', occurrence_id: selectedForScope } : { kind: 'thermal_object', design_id: boundDesigns[0] ?? '', object_id: 'model:building' }; })}><option value="study">{candidate.schema_version==='greenlens.architectural-scope.v2'?'Unbound study note — Missing until linked':'Legacy study note (v1 semantics)'}</option>{candidate.schema_version==='greenlens.architectural-scope.v2'&&<><option value="study_field">Exact study field</option><option value="calendar_field">Exact calendar field</option></>}<option value="reference_occurrence">Selected reference occurrence</option><option value="thermal_object">Thermal object / field</option></select></label></div>
          {a.link.kind === 'reference_occurrence' && <p>Occurrence <code>{a.link.occurrence_id || 'Choose a reference occurrence first'}</code><button disabled={!selectedForScope} onClick={() => changeAssumption(a.id, row => { row.link = { kind: 'reference_occurrence', occurrence_id: selectedForScope }; })}>Use selected occurrence</button></p>}
          {(a.link.kind==='study_field'||a.link.kind==='calendar_field')&&<><label>Bound input design<select value={a.link.design_id} onChange={e=>changeAssumption(a.id,row=>{if(row.link.kind==='study_field'||row.link.kind==='calendar_field')row.link={...row.link,design_id:e.target.value};})}>{boundDesigns.map(id=><option key={id}>{id}</option>)}</select></label><p>{a.link.kind==='calendar_field'?'Calendar pointers include /start_weekday, /utc_offset_minutes, or an exact /holidays/0/month and /holidays/0/day.':'Study pointers include /weather/original_sha256, /weather/normalized_sha256, /weather/time_axis_sha256, /weather/profile_id and /zone_timesteps_per_hour.'} Rust requires an existing scalar field in the exact accepted snapshot. The source declaration stays separate from its applied value.</p></>}
          {a.link.kind === 'thermal_object' && <label>Thermal input object<select value={`${a.link.design_id}/${a.link.object_id}`} onChange={e => { const [design_id, ...object] = e.target.value.split('/'); changeAssumption(a.id, row => { row.link = { kind: 'thermal_object', design_id, object_id: object.join('/') }; }); }}>{boundDesigns.map(id => <option key={`${id}/model:building`} value={`${id}/model:building`}>{id} · Building / site</option>)}{allTargets.filter(t => boundDesigns.includes(t.design_id)).map(t => <option key={`${t.design_id}/${t.object_id}`} value={`${t.design_id}/${t.object_id}`}>{t.design_id} · {t.label} · {t.object_id}</option>)}</select></label>}
          <div className="reference-form-grid">{input(a.link.kind === 'thermal_object' ? 'Input field pointer (for example /site)' : a.link.kind==='study_field'||a.link.kind==='calendar_field'?'Exact typed input field pointer':'Input field', a.field, text => changeAssumption(a.id, row => { row.field = text; }))}{input('Units', a.units, text => changeAssumption(a.id, row => { row.units = text; }))}{a.value !== null && input('Declared value', a.value, text => changeAssumption(a.id, row => { row.value = text; }))}{input('Source / citation', a.source, text => changeAssumption(a.id, row => { row.source = text; }))}</div>{area('Assumption rationale', a.rationale, text => changeAssumption(a.id, row => { row.rationale = text; }))}{input('Affected analysis scope', a.affected_scope, text => changeAssumption(a.id, row => { row.affected_scope = text; }))}<button onClick={() => updateScope(v => { v.assumptions = v.assumptions.filter(row => row.id !== a.id); })}>Remove {a.id}</button>
        </fieldset>)}
      </details>

      <details><summary>Source measurements and approximation reconciliation ({candidate.reconciliations.length})</summary>
        <p>Enter independently measured or documented source values. Rust derives the analytical values. A bounding box is not a source floor-area denominator.</p>
        <button disabled={!selectedForScope || !boundDesigns.length} onClick={() => updateScope(v => { v.reconciliations.push({ id: nextId('measure', v.reconciliations.map(r => r.id)), design_id: boundDesigns[0], occurrence_ids: [selectedForScope], analytical_object_ids: [], measure: 'floor_area', source_measurement: null, method: '' }); })}>Add measurement for selected occurrence</button>
        {candidate.reconciliations.map(r => <fieldset key={r.id} data-scope-key={`reconciliation:${r.id}`}><legend>{r.id}</legend><div className="reference-form-grid"><label>Design<select value={r.design_id} onChange={e => changeReconciliation(r.id, row => { row.design_id = e.target.value; row.analytical_object_ids = row.measure === 'north_rotation' ? ['model:building'] : []; })}>{boundDesigns.map(id => <option key={id}>{id}</option>)}</select></label><label>Measurement<select value={r.measure} onChange={e => changeReconciliation(r.id, (row, d) => { row.measure = e.target.value as ReconciliationMeasure; row.analytical_object_ids = row.measure === 'north_rotation' ? ['model:building'] : []; if (row.source_measurement) row.source_measurement.unit = measures[row.measure][1]; delete d.numbers[`measure.${row.id}`]; })}>{Object.entries(measures).map(([id, [label, , unit]]) => <option key={id} value={id}>{label} ({unit})</option>)}</select></label></div>
          <p>Source occurrence IDs: {r.occurrence_ids.map(short).join(', ')} <button disabled={!selectedForScope || r.occurrence_ids.includes(selectedForScope)} onClick={() => changeReconciliation(r.id, row => { row.occurrence_ids.push(selectedForScope); })}>Add selected occurrence to measurement</button></p>
          {r.measure !== 'north_rotation' && <fieldset><legend>Analytical zones</legend>{context?.designs[r.design_id]?.targets.filter(t => t.kind === 'zone').map(t => <label key={t.object_id}><input type="checkbox" checked={r.analytical_object_ids.includes(t.object_id)} onChange={e => changeReconciliation(r.id, row => { row.analytical_object_ids = row.analytical_object_ids.filter(id => id !== t.object_id); if (e.target.checked) row.analytical_object_ids.push(t.object_id); })} />{t.label} · {t.object_id}</label>)}</fieldset>}
          <label><input type="checkbox" checked={r.source_measurement !== null} onChange={e => changeReconciliation(r.id, (row, d) => { row.source_measurement = e.target.checked ? { value: 0, unit: measures[row.measure][1], source: '', method: '' } : null; if (e.target.checked) d.numbers[`measure.${row.id}`] = ''; else delete d.numbers[`measure.${row.id}`]; })} /> A source measurement is available</label>
          {r.source_measurement && <div className="reference-form-grid">{input(`Source value (${measures[r.measure][2]})`, controlled.scope?.numbers[`measure.${r.id}`] ?? String(r.source_measurement.value), text => changeReconciliation(r.id, (_row, d) => { d.numbers[`measure.${r.id}`] = text; }), true)}{input('Measurement source', r.source_measurement.source, text => changeReconciliation(r.id, row => { if (row.source_measurement) row.source_measurement.source = text; }))}{input('Source measurement method', r.source_measurement.method, text => changeReconciliation(r.id, row => { if (row.source_measurement) row.source_measurement.method = text; }))}</div>}
          {area('Selected scope / abstraction method', r.method, text => changeReconciliation(r.id, row => { row.method = text; }))}<button onClick={() => updateScope(v => { v.reconciliations = v.reconciliations.filter(row => row.id !== r.id); })}>Remove {r.id}</button>
        </fieldset>)}
      </details>

      <details><summary>Coverage denominator and whole-building claim</summary><label>Coverage basis<select value={candidate.coverage.status} onChange={e => updateScope((v, d) => { v.coverage = e.target.value === 'unknown' ? { status: 'unknown', reason: '' } : { status: 'declared_floor_area', included_m2: 0, whole_building_m2: 0, source: '', method: '' }; d.numbers['coverage.included'] = ''; d.numbers['coverage.whole'] = ''; })}><option value="unknown">Unknown — no defensible denominator</option><option value="declared_floor_area">Explicit source floor areas</option></select></label>
        {candidate.coverage.status === 'unknown' ? input('Why coverage is unknown', candidate.coverage.reason, text => updateScope(v => { if (v.coverage.status === 'unknown') v.coverage.reason = text; })) : <><div className="reference-form-grid">{input('Included source floor area (m²)', controlled.scope.numbers['coverage.included'] ?? String(candidate.coverage.included_m2), text => updateScope((_v, d) => { d.numbers['coverage.included'] = text; }), true)}{input('Whole-building source floor area (m²)', controlled.scope.numbers['coverage.whole'] ?? String(candidate.coverage.whole_building_m2), text => updateScope((_v, d) => { d.numbers['coverage.whole'] = text; }), true)}{input('Coverage source', candidate.coverage.source, text => updateScope(v => { if (v.coverage.status === 'declared_floor_area') v.coverage.source = text; }))}</div>{area('Coverage measurement method', candidate.coverage.method, text => updateScope(v => { if (v.coverage.status === 'declared_floor_area') v.coverage.method = text; }))}</>}
        {candidate.scope_kind === 'whole_building' && <>{area('Whole-building review evidence', candidate.whole_building_review ?? '', text => updateScope(v => { v.whole_building_review = text || null; }))}<p>Rust rejects a whole-building claim without complete occurrence classification, zone mappings, measured coverage and floor-area reconciliation. Unsupported geometry cannot be made equivalent by this declaration.</p></>}
      </details>
      {!candidateMode&&<div className="reference-actions"><button onClick={() => void review()}>Review scope in Rust</button><button disabled={!preview || preview.relationship !== 'current' || previewSignature.current !== signature} onClick={() => void save()}>Save accepted scope</button><button onClick={e => requestConfirmation('discard', e.currentTarget)}>Discard scope draft…</button></div>}
    </fieldset>}
    {preview && <section aria-label="Rust scope preview"><h3>Rust scope preview</h3><Assessment value={preview} /></section>}
    <p role="status">{busy ? 'Checking reference action…' : 'Simulation inclusion follows accepted analytical inputs. Reference visibility never changes it.'}</p>
  </section>;
}

function Assessment({ value }: { value: ScopeAssessment }) {
  return <><p>Scope relationship: <strong>{value.relationship}</strong>. Coverage: {value.coverage_percent === null ? 'Unknown' : `${value.coverage_percent}% (declared source floor area)`}. Unresolved occurrences: {value.unresolved_mapping_count}; missing assumptions: {value.missing_assumption_count}.</p>{value.stale_reasons.map((issue, i) => <p role="alert" key={i}>{issue.field}: {issue.message}</p>)}<p>{value.disclosure}</p>{value.reconciliations.length > 0 && <div className="reference-table"><table><caption>Rust-derived reconciliation; unknown source values stay absent</caption><thead><tr><th>Measurement</th><th>Source</th><th>Analytical</th><th>Difference</th><th>Difference %</th><th>Units</th></tr></thead><tbody>{value.reconciliations.map(r => <tr key={r.id}><th>{r.design_id} · {r.id}</th><td>{r.source_value ?? 'Unknown'}</td><td>{r.analytical_value}</td><td>{r.analytical_minus_source ?? 'Unknown'}</td><td>{r.relative_difference_percent ?? 'N/A'}</td><td>{r.unit.replaceAll('_', ' ')}</td></tr>)}</tbody></table></div>}</>;
}
