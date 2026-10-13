// Mirrors the versioned Rust energy wire contracts. Rust owns validation and arithmetic.
export interface EnergyDesign { model: ThermalModel; study: EnergyStudy | null }
export interface EnergyProject { schema_version: string; designs: Record<'A'|'B',EnergyDesign>; weather: {id:string;path:string;binding:EnergyWeatherBinding}[]; runs: EnergyRunSummary[]; carbon_factor: CarbonFactor|null; lighting_service_evidence:string|null; attachments?:EnergyAttachment[] }
export interface EnergyRunSummary {id:string; alternative_id:string; path:string; physical_input_hash:string; record_sha256:string}
export interface EnergySeriesManifest {interval_count:number; interval_minutes:number; channels:{id:string;label:string;unit:string}[]}
export interface EnergyRunRecord {id:string; alternative_id:string;model:ThermalModel;study:EnergyStudy;validated:ValidatedThermalModel;method:EnergyMethodIdentity;ledger:MeterLedger;series:EnergySeriesManifest;totals:EnergyTotals;provenance:{physical_input_hash:string;processing_identity:string;diagnostics:string[]}}
export interface ValidatedThermalModel {model:ThermalModel;surfaces:{id:string;zone_id:string;face:ThermalFace;vertices_m:[number,number,number][];outward_normal:[number,number,number];gross_area_m2:number;opening_area_m2:number}[];gross_floor_area_m2:number;conditioned_floor_area_m2:number;construction_u_values_w_m2k:Record<string,number>;facade_wwr:{orientation:string;gross_wall_m2:number;opening_m2:number;ratio:number|null}[];model_input_sha256:string}
export interface EnergyRows {run_id:string;series:EnergySeriesManifest;start:number;rows:number[][];demand_10min:{timestamp:EnergyTimestamp;facility_kw:number;end_uses_kw:Record<EnergyEndUse,number>}[];integrity_scope:string}
export type BalancedVentilation = { design_flow_m3_s: number; fraction_schedule: string; fan_pressure_pa: number; total_efficiency: number };
export type CarbonFactor = { id: string; fuel: Fuel; region: string; year: number; kg_per_kwh: number; unit: EmissionUnit; source: string; version: string; synthetic: boolean };
export type CoincidentDemand = { timestamp: EnergyTimestamp; facility_kw: number; end_use_contributions_kw: Record<EnergyEndUse, number>; reconciliation_residual_kw: number };
export type ComfortSummary = { zone_id?: string | null; occupied_hours: number; cooling_unmet_hours: number; unmet_percent?: number | null };
export type CurveForm = { kind: "quadratic"; coefficients: [number, number, number]; x: [number, number] } | { kind: "cubic"; coefficients: [number, number, number, number]; x: [number, number] } | { kind: "biquadratic"; coefficients: [number, number, number, number, number, number]; x: [number, number]; y: [number, number] };
export type DayProfile = { values: (number)[] };
export type DxCoolingSystem = { id: string; gross_rated_capacity_w: number; gross_rated_cop: number; rated_shr: number; rated_airflow_m3_s: number; rated_indoor_wet_bulb_c: number; rated_outdoor_dry_bulb_c: number; capacity_temperature_curve: string; capacity_flow_curve: string; eir_temperature_curve: string; eir_flow_curve: string; part_load_fraction_curve: string; availability_schedule: string; cooling_setpoint_schedule: string; cooling_deadband_k: number; supply_fan: SupplyFan; balanced_ventilation: BalancedVentilation; sizing_source: InputSource };
export type EmissionUnit = "kg_co2" | "kg_co2e";
export type EnergyAssessment = { execution: EnergyExecutionStatus; evidence_integrity: EnergyIntegrityScope; current_physical_inputs: EnergyInputRelation; current_processed_results: EnergyInputRelation; current_derived_results: EnergyInputRelation; scientific: EnergyQualification };
export type EnergyCalendar = { start_weekday: Weekday; holidays: (MonthDay)[]; utc_offset_minutes: number };
export type EnergyCasePresentation = { case_id: string; annual: EnergyPeriodPresentation; monthly: (EnergyPeriodPresentation)[]; peak_kw: ReportCell; peak_end_uses_kw: Record<EnergyEndUse, ReportCell>; peak_timestamp: EnergyTimestamp; peak_reconciliation_residual: ReportCell; maximum_interval_reconciliation_residual: ReportCell; gross_floor_area: ReportCell; conditioned_floor_area: ReportCell; gross_eui: ReportCell; conditioned_eui: ReportCell; emissions: ReportCell; comfort: (EnergyComfortPresentation)[]; building_comfort_union: EnergyComfortPresentation };
export type EnergyComfortPresentation = { zone_id?: string | null; occupied_hours: ReportCell; cooling_unmet_hours: ReportCell; unmet_percent: ReportCell };
export type EnergyComparison = { input_relation: EnergyInputComparison; rows: (EnergyComparisonRow)[]; facility_energy_savings: Savings; facility_peak_savings: Savings; lighting_service_equivalence: string; savings_claim_limitations: (string)[] };
export type EnergyComparisonPresentation = { end_use: EnergyEndUse; baseline_kwh: ReportCell; proposed_kwh: ReportCell; energy_savings: ReportSavings; baseline_coincident_kw: ReportCell; proposed_coincident_kw: ReportCell; demand_savings: ReportSavings };
export type EnergyComparisonRow = { end_use: EnergyEndUse; baseline_kwh: number; proposed_kwh: number; energy_savings: Savings; baseline_coincident_kw: number; proposed_coincident_kw: number; demand_savings: Savings };
export type EnergyEndUse = "cooling_system" | "supply_fans" | "ventilation_exhaust_fans" | "interior_lighting" | "exterior_lighting" | "plug_equipment";
export type EnergyEvidence = { id: string; relative_path: string; sha256: string; caption: string; included: boolean; rights: string };
export type EnergyExecutionStatus = "completed" | "failed" | "cancelled" | "not_executed";
export type EnergyInputComparison = { comparable_geometry_weather_calendar: boolean; unchanged_service_inputs: boolean; differences: (string)[]; service_warnings: (string)[] };
export type EnergyInputRelation = "current" | "changed" | "not_compared";
export type EnergyIntegrityScope = "not_checked" | "metadata_checked" | "complete_retained_evidence_checked";
export type EnergyMethodIdentity = { method_id: string; adapter_version: string; engine_version: string; engine_package_sha256: string; engine_executable_sha256: string; engine_schema_sha256: string; recipe_sha256: string; numerical_settings_sha256: string };
export type EnergyPeriodPresentation = { facility: ReportCell; end_uses: Record<EnergyEndUse, ReportCell>; reconciliation_residual: ReportCell };
export type EnergyPeriodTotal = { facility_kwh: number; end_uses_kwh: Record<EnergyEndUse, number>; reconciliation_residual_kwh: number };
export type EnergyQualification = { status: EnergyQualificationStatus; method: EnergyMethodIdentity; registry_version: string; reason: string; tested_scope: string; applicable_physical_input_sha256: (string)[]; evidence_ids: (string)[] };
export type EnergyQualificationStatus = "unassessed" | "failed" | "limited_qualified";
export type EnergyReportAsset = { id: string; relative_path: string; sha256: string; caption: string; alt_text: string; model_input_sha256: string };
export type EnergyReportCase = { id: string; label: string; model_id: string; model: ThermalModel; study: EnergyStudy; weather_summary: EnergyWeatherSummary; model_input_sha256: string; run_id: string; record_sha256: string; physical_input_sha256: string; processing_sha256: string; method: EnergyMethodIdentity; assessment: EnergyAssessment; gross_floor_area_m2: number; conditioned_floor_area_m2: number; totals: EnergyTotals; meter_ledger: MeterLedger; diagnostics: EnergyReportDiagnostics; carbon_factor?: CarbonFactor | null; limitations: (string)[] };
export type EnergyReportData = { schema_version: "greenlens.energy-report.v0.4"; id: string; title: string; revision: string; scope: string; baseline: EnergyReportCase; proposed: EnergyReportCase; comparison: EnergyComparison; assets: (EnergyReportAsset)[]; evidence: (EnergyEvidence)[]; limitations: (string)[] };
export type EnergyReportDiagnostics = { source_record_sha256: string; messages: (string)[]; warning_count?: number | null; severe_count?: number | null; convergence_disclosure: string };
export type EnergyReportPresentation = { baseline: EnergyCasePresentation; proposed: EnergyCasePresentation; end_uses: (EnergyComparisonPresentation)[]; facility_energy_savings: ReportSavings; facility_peak_savings: ReportSavings };
export type EnergyStudy = { schema_version: "greenlens.energy-study.v0.4"; weather: EnergyWeatherBinding; calendar: EnergyCalendar; zone_timesteps_per_hour: 6 | 12 };
export type EnergyTimestamp = { interval_index: number; month: number; day: number; start_minute: number; end_minute: number };
export type EnergyTotals = { interval_count: 52560 | 105120; interval_seconds: 300 | 600; annual: EnergyPeriodTotal; monthly: (EnergyPeriodTotal)[]; demand_interval_seconds: 600; coincident_peak: CoincidentDemand; zone_comfort: (ComfortSummary)[]; building_comfort_union: ComfortSummary; comfort_definition: string; maximum_interval_reconciliation_residual_kw: number };
export type EnergyUnit = "kwh" | "kw" | "kwh_per_m2" | "m2" | "hours" | "percent" | "kg_co2" | "kg_co2e" | "celsius" | "watts" | "m3_per_second" | "dimensionless";
export type EnergyWeatherBinding = { original_sha256: string; normalized_sha256: string; time_axis_sha256: string; profile_id: "greenlens.energy-epw-explicit-fields.v1" };
export type EnergyWeatherSummary = { binding: EnergyWeatherBinding; location: WeatherLocation; source: InputSource; year_hours: 8760 };
export type Fuel = "electricity";
export type GlazingBasis = "whole_window" | "glass_without_frame";
export type HolidayOverride = { date: MonthDay; profile: DayProfile };
export type InfiltrationRate = { basis: "air_changes_per_hour"; value: number } | { basis: "flow_m3_per_second"; value: number } | { basis: "flow_m3_per_second_per_floor_m2"; value: number };
export type InputSource = { citation: string; synthetic: boolean };
export type MeterLedger = { version: string; facility_native_channel: string; entries: (MeterLedgerEntry)[] };
export type MeterLedgerEntry = { end_use: EnergyEndUse; fuel: Fuel; native_channels: (string)[]; boundary: string; classification: ProcessClassification; classification_source?: string | null };
export type MonthDay = { month: number; day: number };
export type OpaqueConstruction = { id: string; name: string; layers: (string)[]; derived_inside_film_m2k_w: number; derived_outside_film_m2k_w: number };
export type PerformanceCurve = { id: string; form: CurveForm; output_limits: [number, number]; source: InputSource };
export type ProcessClassification = "process" | "non_process" | "unclassified";
export type ReportCell = { state: "value"; value: number; unit: EnergyUnit; decimal_places: number; display: string } | { state: "not_applicable"; reason: string } | { state: "missing"; reason: string } | { state: "unsupported"; reason: string };
export type ReportSavings = { absolute: ReportCell; percent: ReportCell };
export type Savings = { absolute: number; percent?: number | null };
export type Schedule = { id: string; kind: ScheduleKind; weekday: DayProfile; saturday: DayProfile; sunday: DayProfile; holidays: (HolidayOverride)[] };
export type ScheduleKind = "fraction" | "activity_watts_per_person" | "temperature_celsius";
export type SupplyFan = { pressure_rise_pa: number; total_efficiency: number; motor_efficiency: number; motor_in_air_fraction: number };
export type ThermalBoundary = { kind: "outdoors" } | { kind: "adiabatic"; reason: string } | { kind: "insulated_ground"; reason: string } | { kind: "interzone"; zone_id: string; surface_id: string };
export type ThermalBoxShade = { id: string; origin_m: [number, number, number]; size_m: [number, number, number] };
export type ThermalFace = "west" | "east" | "south" | "north" | "floor" | "roof";
export type ThermalGlazing = { id: string; name: string; u_value_w_m2k: number; shgc: number; visible_transmittance: number; basis: GlazingBasis; source: InputSource };
export type ThermalMaterial = { id: string; name: string; thickness_m: number; conductivity_w_mk: number; density_kg_m3: number; specific_heat_j_kgk: number; thermal_absorptance: number; solar_absorptance: number; visible_absorptance: number; source: InputSource };
export type ThermalModel = { schema_version: "greenlens.thermal-model.v0.4"; name: string; site: ThermalSite; north_rotation_deg: number; zones: (ThermalZone)[]; materials: (ThermalMaterial)[]; constructions: (OpaqueConstruction)[]; glazing: (ThermalGlazing)[]; curves: (PerformanceCurve)[]; schedules: (Schedule)[]; detached_shading: (ThermalBoxShade)[]; exterior_lighting_w: number; exterior_lighting_schedule: string };
export type ThermalOverhang = { projection_m: number; side_extension_m: number; gap_above_m: number };
export type ThermalSite = { latitude_deg_north: number; longitude_deg_east: number; elevation_m: number };
export type ThermalSurface = { id: string; face: ThermalFace; construction_id: string; boundary: ThermalBoundary };
export type ThermalWindow = { id: string; surface_id: string; glazing_id: string; offset_u_m: number; offset_v_m: number; width_m: number; height_m: number; overhang?: ThermalOverhang | null };
export type ThermalZone = { id: string; name: string; storey: 1 | 2; origin_m: [number, number, number]; size_m: [number, number, number]; surfaces: [ThermalSurface, ThermalSurface, ThermalSurface, ThermalSurface, ThermalSurface, ThermalSurface]; windows: (ThermalWindow)[]; gains: ZoneGains; cooling_system?: DxCoolingSystem | null };
export type WeatherLocation = { city: string; region: string; country: string; source: string; station: string; latitude_deg_north: number; longitude_deg_east: number; utc_offset_minutes: number; elevation_m: number };
export type Weekday = "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday" | "sunday";
export type ZoneGains = { people_per_floor_m2: number; occupancy_schedule: string; activity_schedule: string; people_sensible_fraction: number; people_radiant_fraction_of_sensible: number; lighting_w_per_floor_m2: number; lighting_schedule: string; lighting_radiant_fraction: number; lighting_visible_fraction: number; lighting_return_air_fraction: number; equipment_w_per_floor_m2: number; equipment_schedule: string; equipment_radiant_fraction: number; equipment_latent_fraction: number; equipment_lost_fraction: number; infiltration: InfiltrationRate; infiltration_schedule: string; source: InputSource };

export interface EnergyAttachment {id:string;path:string;sha256:string;media_type:string;model_input_sha256:string;caption:string;rights:string;include_in_export:boolean}
export type EnergyMetadataChange={kind:'attachment';id:string;included:boolean}|{kind:'report_settings';carbon_factor:CarbonFactor|null;lighting_service_evidence:string|null};
