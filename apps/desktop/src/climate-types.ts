/** Serialized Rust contracts only. No weather-time, physics or metric arithmetic. */
import type { JobStatus, Model, Project, ProjectView, Statistics, ValidatedModel } from './types';

export type WeatherSourceProfile = 'epw_hour_ending' | 'pvgis_tmy';
export interface WeatherBinding {
  original_sha256: string;
  normalized_sha256: string;
  time_axis_sha256: string;
  interpretation: string;
}
export interface WeatherSource {
  citation: string;
  source_url: string | null;
  license: string;
  rights_note: string;
  synthetic_non_climatic: boolean;
}
export interface WeatherLocation {
  city: string; region: string; country: string; source: string; station: string;
  latitude_deg_north: number; longitude_deg_east: number;
  utc_offset_minutes: number; elevation_m: number;
}
export interface WeatherTimePolicy {
  profile: WeatherSourceProfile; interpretation: string;
  source_minute: number; canonical_minute: number; interval_duration_minutes: number;
  representative_offset_from_interval_end_minutes: number;
  declared_irradiance_offset_hours: number | null;
  fixed_local_standard_time: boolean; source_year_is_calendar: boolean;
  applies_daylight_saving: boolean;
}
export interface WeatherWarning { code: string; count: number; message: string }
export interface WeatherSummary {
  intervals: number; source_years_by_month: number[][]; zero_solar_intervals: number;
  direct_normal_total_wh_m2: number; diffuse_horizontal_total_wh_m2: number;
  global_horizontal_total_wh_m2: number; minimum_dew_point_c: number; maximum_dew_point_c: number;
}
export interface WeatherInspection {
  binding: WeatherBinding; location: WeatherLocation; time_policy: WeatherTimePolicy;
  summary: WeatherSummary; warnings: WeatherWarning[];
}
export interface WeatherAsset {
  schema_version: 'greenlens.weather-asset.v0.3';
  binding: WeatherBinding; profile: WeatherSourceProfile; source: WeatherSource;
}
export interface WeatherAssetInspection { asset: WeatherAsset; inspection: WeatherInspection }
export interface WeatherHour {
  index: number; source_year: number; month: number; day: number;
  hour_ending: number; source_minute: number;
  direct_normal_wh_m2: number; diffuse_horizontal_wh_m2: number;
  global_horizontal_wh_m2: number; dew_point_c: number;
}
/** Minutes on Rust's signed, unwrapped non-leap axis; never JS/OS time conversion. */
export interface WeatherInterval {
  index: number; month: number; day: number; hour_ending: number;
  local_start_minute: number; local_end_minute: number; local_representative_minute: number;
  utc_start_minute: number; utc_end_minute: number; utc_representative_minute: number;
}
export interface WeatherIntervalView {
  weather_id: string; binding: WeatherBinding; hour: WeatherHour; interval: WeatherInterval;
}
export interface RunWeatherIntervalView {
  run_id: string; record_sha256: string; binding: WeatherBinding; hour: WeatherHour; interval: WeatherInterval;
}

export type NumericalProfile = 'shipping' | 'reference';
export type StudyMode =
  | { mode: 'static_overcast' }
  | { mode: 'weather_hour'; weather: WeatherBinding; interval_index: number; profile: NumericalProfile }
  | { mode: 'annual'; weather: WeatherBinding; profile: NumericalProfile };
export type Occupancy =
  | { kind: 'daily'; start_minute: number; end_minute: number }
  | { kind: 'mask'; weights: number[] };
export interface MetricSettings {
  da_threshold_lux: number; udi_lower_lux: number; udi_upper_lux: number; occupancy: Occupancy;
}
export interface Study {
  schema_version: 'greenlens.study.v0.3'; calculation: StudyMode; metrics: MetricSettings;
}
export interface StudyValidation {
  study: Study; physical_input_hash: string; comparison_input_hash: string;
}
export interface StudyHistorySnapshot {
  project_schema_version: string;
  /** Null means the source legacy document had no climate extension. */
  study: Study | null;
}
export interface SensorMetrics {
  da_percent: number; cda_percent: number; udi_percent: number;
  underlit_percent: number; overlit_percent: number;
}
export interface WeatherReference { id: string; path: string; binding: WeatherBinding }
export interface ClimateRunSummary {
  id: string; alternative_id: string; path: string;
  physical_input_hash: string; comparison_fingerprint: string; record_sha256: string;
}
export interface ClimateProject {
  schema_version: 'greenlens.climate-project.v0.3';
  weather: WeatherReference[]; studies: Record<string, Study>; runs: ClimateRunSummary[];
}
export interface SeriesChunk {
  file: string; start: number; intervals: number; bytes: number; sha256: string;
}
/** Compact chunk metadata only; raw annual arrays are never returned here. */
export interface TimeSeriesManifest {
  schema_version: 'greenlens.timeseries.v0.3'; encoding: 'float64_little_endian';
  ordering: 'time_sensor_channel'; channels: string[]; units: string[];
  sensor_ids: string[]; interval_count: number; chunks: SeriesChunk[];
}
export interface ClimateRunRecord {
  schema_version: 'greenlens.climate-run.v0.3'; id: string; alternative_id: string;
  model: Model; validated: ValidatedModel; study: Study; weather_asset: WeatherAsset;
  series: TimeSeriesManifest; metrics: SensorMetrics[] | null; provenance: Record<string, unknown>;
}
/** Supplementary Rust assessment; never inserted into immutable run evidence. */
export interface RunAssessment {
  schema_version: string; run_id: string; record_sha256: string; recipe_sha256: string;
  process_completion: 'retained_successful_exit_records';
  evidence_integrity: 'validated_record_only' | 'sealed_metadata_checked' | 'full_retained_evidence_checked';
  current_input_relation: {
    physical: 'matches' | 'differs' | 'not_applicable' | 'not_compared';
    metrics: 'matches' | 'differs' | 'not_applicable' | 'not_compared';
  };
  scientific_qualification: {
    status: 'failed' | 'unassessed' | 'limited_qualified';
    registry_version: string; registry_sha256: string;
    identity: { method_version: string; adapter_version: string; profile: unknown; profile_sha256: string; engine: unknown; engine_sha256: string; recipe_contract_sha256: string; physical_input_hash: string | null; scope: string };
    profile_display_name: string; reason: string;
    evidence: { id: string; sha256: string; tested_scope: string; outcome: string; reason: string }[];
    limitations: string[];
  };
}
export type ClimateProjectDocument = Project & { climate?: ClimateProject | null };
export type ClimateProjectView = Omit<ProjectView, 'project'> & {
  project: ClimateProjectDocument; climate_results: ClimateRunRecord[]; climate_assessments: Record<string,RunAssessment>;
};
/** The existing shared job and close/save guards cover both result kinds. */
export type ClimateJobStatus = JobStatus & { climate_result?: ClimateRunRecord | null };
export interface CurrentClimateResult {
  assessment: RunAssessment | null;
  physical_input_hash: string; summary: ClimateRunSummary | null; run: ClimateRunRecord | null;
  stored_metric_identity: string | null; requested_metric_identity: string | null;
  /** False when no matching annual metrics exist, including selected-hour mode. */
  metrics_current: boolean;
}
export type ResultField = 'illuminance' | 'da' | 'cda' | 'udi' | 'underlit' | 'overlit';
export interface ClimateValues {
  assessment: RunAssessment;
  run_id: string; field: ResultField; units: 'lx' | '% of occupied time';
  interval_index: number | null; metric_identity: string | null;
  sensor_ids: string[]; values: number[]; statistics: Statistics;
}
export interface ClimateSensorSeries {
  run_id: string; sensor_id: string; units: 'lx'; time_axis_sha256: string;
  first_interval_index: number;
  /** One sensor's 1 or 8,760 illuminances, not a sensor-by-time matrix. */
  values: number[];
}
export interface DerivedMetrics {
  assessment: RunAssessment;
  run_id: string; metric_identity: string; settings: MetricSettings; metrics: SensorMetrics[];
}
export interface ClimateComparison {
  a: ClimateValues; b: ClimateValues; deltas: number[];
  delta_units: 'lx' | 'percentage points'; delta_statistics: Statistics;
  scale_maximum: number; conditions: unknown; design_differences: unknown;
}

/** Invoke argument names are camelCase; serialized Rust result fields stay snake_case. */
export interface ClimateCommands {
  climate_assessment: {
    args: { path: string; project: ClimateProjectDocument; runId: string }; result: RunAssessment;
  };
  export_project: {
    args: { path: string; destination: string; selectedRuns?: string[] | null };
    result: { path: string };
  };
  dialog_import_epw: { args: { initialPath?: string | null }; result: string | null };
  dialog_weather_source: { args: { initialPath?: string | null }; result: string | null };
  weather_inspect: { args: { path: string; profile: WeatherSourceProfile }; result: WeatherInspection };
  weather_read_source: { args: { path: string }; result: WeatherSource };
  weather_import: {
    args: { path: string; destination: string; profile: WeatherSourceProfile; source: WeatherSource };
    result: WeatherInspection;
  };
  weather_attach: {
    args: { path: string; project: ClimateProjectDocument; expectedRevision: string | null; assetPath: string };
    result: ClimateProjectView;
  };
  weather_import_project: {
    args: { path: string; project: ClimateProjectDocument; expectedRevision: string | null; file: string; profile: WeatherSourceProfile; source: WeatherSource };
    result: ClimateProjectView;
  };
  weather_asset_inspect: { args: { path: string; weatherId: string }; result: WeatherAssetInspection };
  weather_interval: {
    args: { path: string; weatherId: string; intervalIndex: number }; result: WeatherIntervalView;
  };
  weather_find_interval: {
    args: { path: string; weatherId: string; month: number; day: number; hourEnding: number }; result: WeatherIntervalView;
  };
  run_weather_interval: {
    args: { path: string; summary: ClimateRunSummary; intervalIndex: number }; result: RunWeatherIntervalView;
  };
  run_weather_find_interval: {
    args: { path: string; summary: ClimateRunSummary; month: number; day: number; hourEnding: number }; result: RunWeatherIntervalView;
  };
  study_defaults: { args: Record<string, never>; result: Study };
  study_validate: { args: { model: Model; study: Study }; result: StudyValidation };
  study_configure: {
    args: { project: ClimateProjectDocument; alternativeId: string; study: Study }; result: ClimateProjectDocument;
  };
  study_restore: {
    args: { project: ClimateProjectDocument; alternativeId: string; snapshot: StudyHistorySnapshot };
    result: ClimateProjectDocument;
  };
  start_study: {
    args: { path: string | null; project: ClimateProjectDocument; alternativeId: string; study: Study; engineDir?: string | null };
    result: ClimateJobStatus;
  };
  current_climate_result: {
    args: { path: string; project: ClimateProjectDocument; alternativeId: string; study: Study };
    result: CurrentClimateResult;
  };
  climate_result_values: {
    args: { path: string; summary: ClimateRunSummary; field: ResultField; intervalIndex: number; settings?: MetricSettings | null };
    result: ClimateValues;
  };
  climate_sensor_series: {
    args: { path: string; summary: ClimateRunSummary; sensorId: string }; result: ClimateSensorSeries;
  };
  climate_derive_metrics: {
    args: { path: string; summary: ClimateRunSummary; settings: MetricSettings }; result: DerivedMetrics;
  };
  climate_compare: {
    args: { path: string; a: ClimateRunSummary; b: ClimateRunSummary; field: ResultField; intervalIndex: number };
    result: ClimateComparison;
  };
}
