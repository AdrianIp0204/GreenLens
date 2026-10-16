import type { ArchitecturalScope, ReferenceImportDraft, ReferenceImportOptions, ReferenceModel, ReferenceScopeDraft, ReferenceWorkbenchDraft, ScopeContext, ReferenceReportViewCapture } from './types';

export const newReferenceDraft = (documentId: string): ReferenceWorkbenchDraft => ({ schema_version: 'greenlens.reference-workbench-draft.v1', document_id: documentId, import: null, scope: null });
export const newImportDraft = (): ReferenceImportDraft => ({ scale: '1', rotation: '0', x: '0', y: '0', z: '0', scene: '', north: '', application: '', version: '', options: '', note: '', dimensions_checked: false, orientation_checked: false });
const number = (value: string, label: string): number => {
  if (!value.trim() || !Number.isFinite(Number(value))) throw new Error(`${label}: enter a finite number; the draft is retained.`);
  return Number(value);
};
/** Parsing only. Accepted bounds, transforms and all scope validation stay in Rust. */
export function parseReferenceImportDraft(value: ReferenceImportDraft): ReferenceImportOptions {
  if (!value.dimensions_checked || !value.orientation_checked) throw new Error('Review the explicit scale and orientation choices before choosing a local GLB.');
  const scene = value.scene.trim() ? number(value.scene, 'Scene index') : null;
  if (scene !== null && !Number.isInteger(scene)) throw new Error('Scene index must be an integer.');
  return {
    scene_index: scene, metres_per_source_unit: number(value.scale, 'Scale'), clockwise_rotation_deg: number(value.rotation, 'Rotation'),
    translation_m: [number(value.x, 'Translation X'), number(value.y, 'Translation Y'), number(value.z, 'Translation Z')],
    declared_north_clockwise_deg: value.north.trim() ? number(value.north, 'True north') : null,
    conversion: [value.application, value.version, value.options, value.note].some(v => v.trim())
      ? { application: value.application, version: value.version, options: value.options, provenance_note: value.note } : null,
  };
}
export function newScopeDraft(reference: ReferenceModel, context: ScopeContext): ReferenceScopeDraft {
  return { value: {
    schema_version: 'greenlens.architectural-scope.v2', title: '', purpose: '', scope_kind: 'partial_building',
    reference_revision_sha256: reference.revision_sha256, reference_geometry_sha256: reference.geometry_sha256,
    reference_options_sha256: context.reference_options_sha256,
    design_model_sha256: Object.fromEntries(Object.entries(context.designs).map(([id, design]) => [id, design.model_input_sha256])),
    mappings: [], included_scope: [], excluded_scope: [], boundary_consequences: '', assumptions: [], reconciliations: [],
    coverage: { status: 'unknown', reason: 'No measured whole-building floor-area denominator has been established.' }, whole_building_review: null,
  }, numbers: {} };
}
export function parseReferenceScopeDraft(draft: ReferenceScopeDraft): ArchitecturalScope {
  const value = structuredClone(draft.value);
  if (value.coverage.status === 'declared_floor_area') {
    value.coverage.included_m2 = number(draft.numbers['coverage.included'] ?? String(value.coverage.included_m2), 'Included source floor area (m²)');
    value.coverage.whole_building_m2 = number(draft.numbers['coverage.whole'] ?? String(value.coverage.whole_building_m2), 'Whole-building source floor area (m²)');
  }
  for (const row of value.reconciliations) {
    if (row.source_measurement) row.source_measurement.value = number(draft.numbers[`measure.${row.id}`] ?? String(row.source_measurement.value), `Source measurement ${row.id}`);
  }
  return value;
}

/** Append only to a controlled scope draft. Existing edits are never accepted,
 * replaced or rebound by a viewport action; Review/Save still run in Rust. */
export function stageReferenceReportView(documentId:string,draft:ReferenceWorkbenchDraft|null,accepted:ArchitecturalScope|null,reference:ReferenceModel,context:ScopeContext,designId:string,capture:ReferenceReportViewCapture):ReferenceWorkbenchDraft {
  if(draft&&draft.document_id!==documentId)throw Error('The reference draft belongs to another document. The view was not staged.');
  if(draft?.import)throw Error('Apply or discard the pending reference import before staging a report view.');
  const next=structuredClone(draft??newReferenceDraft(documentId));
  if(!next.scope)next.scope=accepted?{value:structuredClone(accepted),numbers:{}}:newScopeDraft(reference,context);
  const scope=next.scope.value,model=context.designs[designId]?.model_input_sha256;
  if(scope.reference_revision_sha256!==reference.revision_sha256||scope.reference_geometry_sha256!==reference.geometry_sha256||!model||scope.design_model_sha256[designId]!==model)throw Error('The scope is bound to another reference or analytical model. Review a scope for these inputs before adding a figure; existing edits are retained.');
  const views=scope.report_views??[];if(views.length>=8)throw Error('A scope supports eight portable report views. Remove a view in the scope draft before adding another.');
  let index=1;while(views.some(v=>v.id===`view-${index}`))index++;
  scope.report_views=[...views,{...structuredClone(capture),schema_version:'greenlens.reference-report-view.v1',id:`view-${index}`,design_id:designId,model_input_sha256:model,reference_revision_sha256:reference.revision_sha256,reference_geometry_sha256:reference.geometry_sha256}];
  return next;
}

/** Local recovery admits bounded UI shapes, not accepted scientific inputs.
 * Invalid/incomplete text remains a draft; Rust still decides every Apply. */
export function isReferenceWorkbenchDraft(value: unknown, documentId: string): value is ReferenceWorkbenchDraft {
  const record = (v: unknown): v is Record<string, unknown> => Boolean(v) && typeof v === 'object' && !Array.isArray(v);
  const text = (v: unknown): v is string => typeof v === 'string' && v.length <= 16384;
  const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
  const list = (v: unknown, limit: number, item: (v: unknown) => boolean): v is unknown[] => Array.isArray(v) && v.length <= limit && v.every(item);
  const names = (v: unknown, keys: string[]) => record(v) && keys.every(k => text(v[k]));
  const oneOf = (v: unknown, choices: string[]) => typeof v === 'string' && choices.includes(v);
  const hash = (v: unknown) => typeof v === 'string' && /^[0-9a-f]{64}$/.test(v);
  const measureNames = ['local_length_x', 'local_length_y', 'height', 'floor_area', 'volume', 'exterior_envelope_gross_area', 'opening_area', 'north_rotation'];
  const units = ['metres', 'square_metres', 'cubic_metres', 'degrees_clockwise'];
  if (!record(value) || value.schema_version !== 'greenlens.reference-workbench-draft.v1' || value.document_id !== documentId) return false;
  if (value.import !== null) {
    const row = value.import;
    if (!record(row) || !names(row, ['scale', 'rotation', 'x', 'y', 'z', 'scene', 'north', 'application', 'version', 'options', 'note'])
      || typeof row.dimensions_checked !== 'boolean' || typeof row.orientation_checked !== 'boolean') return false;
  }
  if (value.scope === null) return true;
  const draft = value.scope;
  if (!record(draft) || !record(draft.value) || !record(draft.numbers) || Object.keys(draft.numbers).length > 4100
    || Object.entries(draft.numbers).some(([key, number]) => key.length > 16500 || !text(number))) return false;
  const scope = draft.value;
  if (!['greenlens.architectural-scope.v1','greenlens.architectural-scope.v2'].includes(String(scope.schema_version)) || !names(scope, ['title', 'purpose', 'boundary_consequences'])
    || !oneOf(scope.scope_kind, ['partial_building', 'representative_study', 'whole_building'])
    || ![scope.reference_revision_sha256, scope.reference_geometry_sha256, scope.reference_options_sha256].every(hash)
    || !record(scope.design_model_sha256) || Object.keys(scope.design_model_sha256).length > 2
    || Object.entries(scope.design_model_sha256).some(([id, identity]) => !text(id) || !hash(identity))
    || !list(scope.included_scope, 256, text) || !list(scope.excluded_scope, 256, text)
    || !(scope.whole_building_review === null || text(scope.whole_building_review))) return false;
  if (!list(scope.mappings, 20000, item => record(item) && names(item, ['occurrence_id', 'rationale'])
    && oneOf(item.status, ['mapped', 'context_only', 'excluded', 'unresolved'])
    && list(item.targets, 64, target => record(target) && names(target, ['design_id', 'object_id'])
      && hash(target.model_input_sha256) && oneOf(target.kind, ['zone', 'surface', 'window', 'overhang', 'detached_shading'])))) return false;
  if (!list(scope.assumptions, 2048, item => {
    if (!record(item) || !names(item, ['id', 'field', 'units', 'source', 'rationale', 'affected_scope'])
      || !oneOf(item.status, ['documented', 'measured', 'assumed', 'synthetic', 'missing', 'not_applicable'])
      || !(item.value === null || text(item.value)) || !record(item.link)) return false;
    const link = item.link;
    return link.kind === 'study' || (['study_field','calendar_field'].includes(String(link.kind)) && scope.schema_version==='greenlens.architectural-scope.v2' && text(link.design_id)) || (link.kind === 'reference_occurrence' && text(link.occurrence_id))
      || (link.kind === 'thermal_object' && names(link, ['design_id', 'object_id']));
  })) return false;
  if (!list(scope.reconciliations, 4096, item => record(item) && names(item, ['id', 'design_id', 'method'])
    && list(item.occurrence_ids, 20000, text) && list(item.analytical_object_ids, 20000, text)
    && oneOf(item.measure, measureNames) && (item.source_measurement === null || (record(item.source_measurement)
      && names(item.source_measurement, ['source', 'method']) && oneOf(item.source_measurement.unit, units)
      && finite(item.source_measurement.value))))) return false;
  const point=(v:unknown)=>list(v,3,n=>finite(n)&&Math.abs(n as number)<=1e9)&&(v as unknown[]).length===3;
  if(scope.report_views!==undefined&&!list(scope.report_views,8,v=>record(v)&&v.schema_version==='greenlens.reference-report-view.v1'&&names(v,['id','title','design_id'])&&[v.model_input_sha256,v.reference_revision_sha256,v.reference_geometry_sha256].every(hash)&&[v.eye_m,v.target_m,v.up].every(point)&&finite(v.vertical_span_m)&&finite(v.aspect_ratio)&&(v.section===null||record(v.section)&&oneOf(v.section.axis,['x','y','z'])&&oneOf(v.section.keep,['above','below'])&&finite(v.section.offset_m))))return false;
  const coverage = scope.coverage;
  return record(coverage) && ((coverage.status === 'unknown' && text(coverage.reason))
    || (coverage.status === 'declared_floor_area' && names(coverage, ['source', 'method']) && finite(coverage.included_m2) && finite(coverage.whole_building_m2)));
}
