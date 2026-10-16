export type Vec3 = [number, number, number];
export interface Model {
  schema_version: string; name: string;
  room: { width_m: number; depth_m: number; height_m: number; rotation_deg: number };
  window: { width_m: number; height_m: number; sill_m: number; center_x_m: number };
  materials: { ceiling_reflectance: number; wall_reflectance: number; floor_reflectance: number; visible_transmittance: number };
  overhang: null | { projection_m: number; side_extension_m: number; gap_above_window_m: number; reflectance: number };
  obstruction: null | { center_x_m: number; width_m: number; depth_m: number; height_m: number; distance_m: number; reflectance: number };
  sensors: { workplane_height_m: number; requested_spacing_m: number; wall_inset_m: number };
  sky: { exterior_horizontal_illuminance_lux: number; ground_reflectance: number };
  metadata: null | { location: string | null; iana_timezone: string | null };
}
export interface Sensor { id: string; position: Vec3; direction: Vec3 }
export interface Surface { id: string; material: 'floor' | 'wall' | 'ceiling' | 'glass' | 'overhang' | 'obstruction'; vertices: Vec3[]; reflectance: number | null }
export interface ValidatedModel {
  model: Model; surfaces: Surface[];
  sensor_grid: { sensors: Sensor[]; columns: number; rows: number; actual_spacing_x_m: number; actual_spacing_y_m: number; cell_area_m2: number };
  normalized_input_hash: string; scientific_input_hash: string; sensor_hash: string; comparison_conditions_hash: string;
}
export interface Statistics { min: number; mean: number; max: number; count: number }
export interface RunRecord { id: string; alternative_id: string; model: Model; validated: ValidatedModel; lux: number[]; statistics: Statistics; provenance: Record<string, unknown> }
export interface RunSummary { id: string; alternative_id: string; path: string; scientific_input_hash: string; comparison_fingerprint: string }
export type DisplayState = import('./viewport/types').ViewportDisplayState;
export interface Project { schema_version: string; id: string; name: string; alternatives: { id: string; model: Model }[]; runs: RunSummary[]; display_state?: DisplayState | null; energy?: import('./energy/types').EnergyProject|null; architecture?:import('./reference/types').ArchitecturalProject|null;report_issues?:import('./energy/report-issues').ReportIssueIndex|null }
export interface ProjectView { project: Project; revision: string; results: RunRecord[]; energy_results?:import('./energy/types').EnergyRunRecord[]; energy_assessments?:Record<string,import('./energy/types').EnergyAssessment>;project_verification?:import('./workspace/project-intake').ProjectVerificationReceipt|null;energy_sensitivity_sessions?:Record<string,import('./energy/EnergySensitivity').Session> }
export interface JobStatus { id: string; state: 'queued' | 'preparing' | 'running' | 'completed' | 'failed' | 'cancelled'; log: string[]; error: string | null; result: RunRecord | null; energy_result?:import('./energy/types').EnergyRunRecord|null; pending_evidence: boolean }
export interface Comparison { a_id: string; b_id: string; deltas_lux: number[]; statistics: Statistics; scale_max_lux: number; conditions: unknown; design_differences: unknown }
