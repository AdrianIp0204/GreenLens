/** Serialized Rust contracts. These types do not implement geometry or physics. */
export type ReferencePoint = [number, number, number];
export type ReferenceImportOptions = {
  scene_index: number | null;
  metres_per_source_unit: number;
  clockwise_rotation_deg: number;
  translation_m: ReferencePoint;
  declared_north_clockwise_deg: number | null;
  conversion: { application: string; version: string; options: string; provenance_note: string } | null;
};
export type ReferenceBounds = { min_m: ReferencePoint; max_m: ReferencePoint };
export type ReferenceOccurrence = {
  id: string; node_index: number; parent_id: string | null; label: string; node_path: number[];
  source_mesh_index: number | null; world_from_mesh: number[]; mirrored: boolean;
  bounds: ReferenceBounds | null; triangle_count: number; triangle_area_m2: number;
};
export type ReferenceTriangle = { occurrence_index: number; material_index: number | null; vertices_m: [ReferencePoint, ReferencePoint, ReferencePoint] };
export type ReferenceModel = {
  schema_version: string; importer_id: string; source_sha256: string; revision_sha256: string; geometry_sha256: string;
  options: ReferenceImportOptions; source_units: string; coordinate_convention: string; selected_scene: number;
  source_node_count: number; source_mesh_count: number; source_material_count: number; source_texture_count: number;
  occurrences: ReferenceOccurrence[];
  materials: { source_index: number; label: string; base_color_rgba: [number, number, number, number]; double_sided: boolean; alpha_mode: string }[];
  chunks: { path: string; sha256: string; byte_length: number; triangle_count: number }[];
  bounds: ReferenceBounds; triangle_count: number; triangle_area_m2: number;
  warnings: string[]; exclusions: string[]; storage_policy: string;
  resources: { elapsed_ms: number; source_bytes: number; json_bytes: number; normalized_bytes: number; decoded_allocation_estimate_bytes: number };
};
export type ReferenceProgress = { stage: string; completed: number; total: number; processed_bytes: number };
export type ReferenceMeasurement = { distance_m: number; delta_m: ReferencePoint; points_m: [ReferencePoint, ReferencePoint]; revision_sha256: string };
export type MappingStatus = 'mapped' | 'context_only' | 'excluded' | 'unresolved';
export type AnalyticalObjectKind = 'zone' | 'surface' | 'window' | 'overhang' | 'detached_shading';
export type AnalyticalTarget = { design_id: string; object_id: string; kind: AnalyticalObjectKind; model_input_sha256: string };
export type ScopeMapping = { occurrence_id: string; status: MappingStatus; targets: AnalyticalTarget[]; rationale: string };
export type AssumptionStatus = 'documented' | 'measured' | 'assumed' | 'synthetic' | 'missing' | 'not_applicable';
export type AssumptionLink = { kind: 'study' } | { kind: 'study_field' | 'calendar_field'; design_id: string } | { kind: 'reference_occurrence'; occurrence_id: string } | { kind: 'thermal_object'; design_id: string; object_id: string };
export type ScopeAssumption = { id: string; link: AssumptionLink; field: string; status: AssumptionStatus; value: string | null; units: string; source: string; rationale: string; affected_scope: string };
export type ReconciliationMeasure = 'local_length_x' | 'local_length_y' | 'height' | 'floor_area' | 'volume' | 'exterior_envelope_gross_area' | 'opening_area' | 'north_rotation';
export type ScopeUnit = 'metres' | 'square_metres' | 'cubic_metres' | 'degrees_clockwise';
export type SourceMeasurement = { value: number; unit: ScopeUnit; source: string; method: string };
export type ScopeReconciliation = { id: string; design_id: string; occurrence_ids: string[]; analytical_object_ids: string[]; measure: ReconciliationMeasure; source_measurement: SourceMeasurement | null; method: string };
export type ScopeCoverage = { status: 'unknown'; reason: string } | { status: 'declared_floor_area'; included_m2: number; whole_building_m2: number; source: string; method: string };
export type ReferenceReportViewCapture = {
  title:string; eye_m:ReferencePoint; target_m:ReferencePoint; up:ReferencePoint;
  vertical_span_m:number; aspect_ratio:number;
  section:{axis:'x'|'y'|'z';offset_m:number;keep:'above'|'below'}|null;
};
export type ReferenceReportView = ReferenceReportViewCapture & {
  schema_version:'greenlens.reference-report-view.v1'; id:string; design_id:string; model_input_sha256:string;
  reference_revision_sha256:string; reference_geometry_sha256:string;
};
export type ArchitecturalScope = {
  schema_version: 'greenlens.architectural-scope.v1' | 'greenlens.architectural-scope.v2'; title: string; purpose: string;
  scope_kind: 'partial_building' | 'representative_study' | 'whole_building';
  reference_revision_sha256: string; reference_geometry_sha256: string; reference_options_sha256: string;
  design_model_sha256: Record<string, string>; mappings: ScopeMapping[];
  included_scope: string[]; excluded_scope: string[]; boundary_consequences: string;
  assumptions: ScopeAssumption[]; reconciliations: ScopeReconciliation[]; coverage: ScopeCoverage;
  whole_building_review: string | null;
  report_views?:ReferenceReportView[];
};
export type ScopeAssessment = {
  schema_version: string; scope_sha256: string; report_binding_sha256: string; relationship: 'current' | 'stale';
  stale_reasons: { field: string; message: string }[]; coverage_percent: number | null;
  reconciliations: { id: string; design_id: string; unit: ScopeUnit; source_value: number | null; analytical_value: number; analytical_minus_source: number | null; relative_difference_percent: number | null }[];
  unresolved_mapping_count: number; missing_assumption_count: number; disclosure: string;
};
export type ScopeContext = {
  reference_options_sha256: string;
  designs: Record<string, { model_input_sha256: string; targets: { object_id: string; kind: AnalyticalObjectKind; label: string }[] }>;
};
export type ReferenceAsset = { id: string; path: string; revision_sha256: string; manifest_sha256: string; geometry_sha256: string };
export type InputProvenanceRow={id:string;design_id:string;object:{kind:string;id:string};field_path:string;applied_value:unknown;units:string|null;declared_source:{citation:string;synthetic:boolean}|null;status:'unreviewed'|'documented'|'measured'|'assumed'|'synthetic'|'missing'|'not_applicable'|'conflicting';source_declarations:ScopeAssumption[]};
export type ScopeRevision={schema_version:'greenlens.scope-revision.v1'|'greenlens.scope-revision.v2';revision_sha256:string;parent_revision_sha256:string|null;reference:{asset_id:string;manifest_sha256:string;manifest:ReferenceModel};designs:Record<string,{model:import('../energy/types').ThermalModel;calendar:import('../energy/types').EnergyCalendar;study:import('../energy/types').EnergyStudy|null}>;scope:ArchitecturalScope;carry_forward_plan_sha256:string|null;review_decisions:ScopeReviewDecision[];provenance:{schema_version:string;rows:InputProvenanceRow[];unreviewed_count:number;missing_count:number;conflicting_count:number;documentation_complete:boolean;disclosure:string}};
export type ScopeReviewDecision={row_id:string;resolution:'reviewed'|'unresolved';rationale:string};
export type ScopeCarryForwardPlan={schema_version:'greenlens.scope-carry-forward.v1'|'greenlens.scope-carry-forward.v2';parent_revision_sha256:string;target_context_sha256:string;plan_sha256:string;proposed_scope:ArchitecturalScope;candidate_scope?:ArchitecturalScope|null;rows:{id:string;kind:'scope'|'reference'|'mapping'|'assumption'|'reconciliation'|'report_view'|'input';key:string;change:'unchanged'|'changed'|'deleted'|'added'|'unresolved';requires_review:boolean;before:unknown;after:unknown;reason:string}[]};
export type ArchitecturalProject = { schema_version: string; assets: ReferenceAsset[]; active_asset_id: string; scope: ArchitecturalScope | null;scope_history?:{schema_version:'greenlens.scope-history.v1';revisions:ScopeRevision[];active_revision_sha256:string|null}|null };

/** Controlled document draft: numeric text survives invalid edits and navigation. */
export type ReferenceImportDraft = {
  scale: string; rotation: string; x: string; y: string; z: string; scene: string; north: string;
  application: string; version: string; options: string; note: string;
  dimensions_checked: boolean; orientation_checked: boolean;
};
export type ReferenceScopeDraft = { value: ArchitecturalScope; numbers: Record<string, string> };
export type ReferenceWorkbenchDraft = {
  schema_version: 'greenlens.reference-workbench-draft.v1'; document_id: string;
  import: ReferenceImportDraft | null; scope: ReferenceScopeDraft | null;
};
