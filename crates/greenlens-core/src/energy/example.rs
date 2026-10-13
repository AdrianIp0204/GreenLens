//! Original synthetic inputs for demonstrations and arithmetic/topology tests.
//! Coefficients are deliberately simple, not calibrated equipment performance.
use super::*;

/// Artificial complete-year arithmetic only. Never used as a native acceptance
/// record; the execution and integrity axes explicitly say it was not executed.
pub fn synthetic_energy_report() -> crate::Result<EnergyReportData> {
    let weather = weather::parse_energy_epw(&weather::synthetic_energy_epw())?.data;
    let study = EnergyStudy {
        schema_version: ENERGY_STUDY_SCHEMA.into(),
        weather: weather.binding(),
        calendar: synthetic_energy_calendar(),
        zone_timesteps_per_hour: 6,
    };
    let h = "0".repeat(64);
    let method = EnergyMethodIdentity {
        method_id: "synthetic-arithmetic-only".into(),
        adapter_version: "none".into(),
        engine_version: "not-executed".into(),
        engine_package_sha256: h.clone(),
        engine_executable_sha256: h.clone(),
        engine_schema_sha256: h.clone(),
        recipe_sha256: h.clone(),
        numerical_settings_sha256: h.clone(),
    };
    let uses = [
        EnergyEndUse::CoolingSystem,
        EnergyEndUse::SupplyFans,
        EnergyEndUse::VentilationExhaustFans,
        EnergyEndUse::InteriorLighting,
        EnergyEndUse::ExteriorLighting,
        EnergyEndUse::PlugEquipment,
    ];
    let ledger = MeterLedger {
        version: "original-synthetic-disjoint.v1".into(),
        facility_native_channel: "synthetic-facility-electricity".into(),
        entries: uses
            .iter()
            .map(|&u| MeterLedgerEntry {
                end_use: u,
                fuel: Fuel::Electricity,
                native_channels: vec![format!("synthetic-{u:?}-electricity")],
                boundary: "Original synthetic disjoint electrical component.".into(),
                classification: ProcessClassification::Unclassified,
                classification_source: None,
            })
            .collect(),
    };
    let mut cases = Vec::new();
    for proposed in [false, true] {
        let model = synthetic_four_zone_model(proposed);
        validate_weather_context(&model, &study, &weather)?;
        let validated = validate_thermal_model(&model, &study.calendar)?;
        let physical = study.physical_input_hash(&model)?;
        let ids = model.zones.iter().map(|z| z.id.clone()).collect::<Vec<_>>();
        let mut accumulator = EnergyAccumulator::new(&ledger, &ids)?;
        for i in 0..ENERGY_INTERVALS {
            let scale = if proposed { 0.8 } else { 1. };
            let end_uses_j = uses.iter().map(|&u| (u, 100_000. * scale)).collect();
            accumulator.push(&EnergyInterval {
                index: i,
                facility_j: 600_000. * scale,
                end_uses_j,
                comfort: ids
                    .iter()
                    .map(|id| ComfortSample {
                        zone_id: id.clone(),
                        occupied: true,
                        air_temperature_c: 24.,
                        cooling_setpoint_c: 24.,
                    })
                    .collect(),
            })?;
        }
        let id = if proposed { "proposed" } else { "baseline" };
        cases.push(EnergyReportCase {
            id: id.into(),
            label: format!("Original synthetic {id}"),
            model_id: id.into(),
            model,
            study: study.clone(),
            weather_summary: EnergyWeatherSummary {
                binding: weather.binding(),
                location: weather.location.clone(),
                source: source(),
                year_hours: 8760,
            },
            model_input_sha256: validated.model_input_sha256,
            run_id: format!("synthetic-{id}-not-a-native-run"),
            record_sha256: h.clone(),
            physical_input_sha256: physical,
            processing_sha256: processing_identity(&h, &ledger)?,
            method: method.clone(),
            assessment: EnergyAssessment {
                execution: EnergyExecutionStatus::NotExecuted,
                evidence_integrity: EnergyIntegrityScope::NotChecked,
                current_physical_inputs: EnergyInputRelation::NotCompared,
                current_processed_results: EnergyInputRelation::NotCompared,
                current_derived_results: EnergyInputRelation::NotCompared,
                scientific: EnergyQualification {
                    status: EnergyQualificationStatus::Unassessed,
                    method: method.clone(),
                    registry_version: "none".into(),
                    reason: "Original synthetic arithmetic and renderer test, not a simulation."
                        .into(),
                    tested_scope: "No native or scientific acceptance.".into(),
                    applicable_physical_input_sha256: vec![],
                    evidence_ids: vec![],
                },
            },
            gross_floor_area_m2: validated.gross_floor_area_m2,
            conditioned_floor_area_m2: validated.conditioned_floor_area_m2,
            totals: accumulator.finish()?,
            meter_ledger: ledger.clone(),
            diagnostics: EnergyReportDiagnostics {
                source_record_sha256: h.clone(),
                messages: vec![],
                warning_count: None,
                severe_count: None,
                convergence_disclosure:
                    "Not available: original synthetic arithmetic; no engine executed.".into(),
            },
            carbon_factor: None,
            limitations: vec![
                "Artificial data; do not use as building performance evidence.".into(),
            ],
        });
    }
    let proposed = cases.pop().expect("two cases");
    let baseline = cases.pop().expect("two cases");
    let relation = compare_energy_inputs(
        &baseline.model,
        &baseline.study,
        &proposed.model,
        &proposed.study,
    )?;
    let comparison=compare_energy_totals(&baseline.totals,&proposed.totals,relation,"Illumination service is unverified; synthetic power reduction proves no lighting service equivalence.".into())?;
    let report = EnergyReportData {
        schema_version: ENERGY_REPORT_SCHEMA.into(),
        id: "original-synthetic-report".into(),
        title: "Original synthetic energy report".into(),
        revision: "1".into(),
        scope: "Arithmetic and rendering only; no engine executed.".into(),
        baseline,
        proposed,
        comparison,
        assets: vec![],
        evidence: vec![],
        limitations: vec![
            "Synthetic unassessed demonstration; no certification or measured savings.".into(),
        ],
    };
    report.validate()?;
    Ok(report)
}

pub fn synthetic_energy_calendar() -> EnergyCalendar {
    EnergyCalendar {
        start_weekday: Weekday::Monday,
        holidays: vec![MonthDay { month: 1, day: 1 }],
        utc_offset_minutes: 0,
    }
}
fn source() -> InputSource {
    InputSource { citation: "Original GreenLens synthetic fixture; illustrative values, not measured climate or product data.".into(), synthetic: true }
}
fn schedule(
    id: &str,
    kind: ScheduleKind,
    weekday: DayProfile,
    weekend: f64,
    holiday: f64,
) -> Schedule {
    Schedule {
        id: id.into(),
        kind,
        weekday,
        saturday: DayProfile::constant(weekend),
        sunday: DayProfile::constant(weekend),
        holidays: vec![HolidayOverride {
            date: MonthDay { month: 1, day: 1 },
            profile: DayProfile::constant(holiday),
        }],
    }
}
pub fn synthetic_four_zone_model(proposed: bool) -> ThermalModel {
    let mut office = DayProfile::constant(0.);
    office.values[48..108].fill(1.);
    let mut evening = DayProfile::constant(0.);
    evening.values[84..132].fill(1.);
    let schedules = vec![
        schedule("office", ScheduleKind::Fraction, office, 0.25, 0.),
        schedule("evening", ScheduleKind::Fraction, evening, 0.5, 0.),
        schedule(
            "always",
            ScheduleKind::Fraction,
            DayProfile::constant(1.),
            1.,
            1.,
        ),
        schedule(
            "activity",
            ScheduleKind::ActivityWattsPerPerson,
            DayProfile::constant(120.),
            120.,
            120.,
        ),
        schedule(
            "setpoint",
            ScheduleKind::TemperatureCelsius,
            DayProfile::constant(24.),
            24.,
            24.,
        ),
    ];
    let material = |id: &str, k, t| ThermalMaterial {
        id: id.into(),
        name: id.into(),
        thickness_m: t,
        conductivity_w_mk: k,
        density_kg_m3: 800.,
        specific_heat_j_kgk: 1000.,
        thermal_absorptance: 0.9,
        solar_absorptance: 0.6,
        visible_absorptance: 0.6,
        source: source(),
    };
    let materials = vec![
        material("masonry", 0.8, 0.2),
        material("insulation", 0.04, if proposed { 0.08 } else { 0.04 }),
    ];
    let constructions = vec![
        OpaqueConstruction {
            id: "external".into(),
            name: "Synthetic layered envelope".into(),
            layers: vec!["masonry".into(), "insulation".into()],
            derived_inside_film_m2k_w: 0.13,
            derived_outside_film_m2k_w: 0.04,
        },
        OpaqueConstruction {
            id: "internal".into(),
            name: "Synthetic symmetric partition".into(),
            layers: vec!["masonry".into()],
            derived_inside_film_m2k_w: 0.13,
            derived_outside_film_m2k_w: 0.13,
        },
    ];
    let curves = vec![
        PerformanceCurve {
            id: "temp_modifier".into(),
            form: CurveForm::Biquadratic {
                coefficients: [1., 0., 0., 0., 0., 0.],
                x: [10., 30.],
                y: [10., 50.],
            },
            output_limits: [0.1, 2.],
            source: source(),
        },
        PerformanceCurve {
            id: "flow_modifier".into(),
            form: CurveForm::Quadratic {
                coefficients: [1., 0., 0.],
                x: [0., 2.],
            },
            output_limits: [0.1, 2.],
            source: source(),
        },
        PerformanceCurve {
            id: "part_load".into(),
            form: CurveForm::Quadratic {
                coefficients: [0.85, 0.15, 0.],
                x: [0., 1.],
            },
            output_limits: [0.85, 1.01],
            source: source(),
        },
    ];
    let mut zones = Vec::new();
    for i in 0..4 {
        let id = format!("zone{}", i + 1);
        let operation = if i % 2 == 0 { "office" } else { "evening" };
        let surfaces=ThermalFace::ALL.into_iter().map(|face| {
            let surface_id=format!("{id}_{face:?}");
            let partner = match (i,face) { (0,ThermalFace::East)=>Some((1,ThermalFace::West)),(1,ThermalFace::West)=>Some((0,ThermalFace::East)),(2,ThermalFace::East)=>Some((3,ThermalFace::West)),(3,ThermalFace::West)=>Some((2,ThermalFace::East)),(0,ThermalFace::Roof)=>Some((2,ThermalFace::Floor)),(2,ThermalFace::Floor)=>Some((0,ThermalFace::Roof)),(1,ThermalFace::Roof)=>Some((3,ThermalFace::Floor)),(3,ThermalFace::Floor)=>Some((1,ThermalFace::Roof)),_=>None };
            ThermalSurface { id:surface_id,face,construction_id:if partner.is_some(){"internal"}else{"external"}.into(),boundary:if let Some((j,f))=partner { ThermalBoundary::Interzone{zone_id:format!("zone{}",j+1),surface_id:format!("zone{}_{f:?}",j+1)} } else if face==ThermalFace::Floor {ThermalBoundary::InsulatedGround{reason:"Explicit synthetic insulated-ground approximation; no ground model.".into()}} else {ThermalBoundary::Outdoors} }
        }).collect();
        zones.push(ThermalZone {
            id: id.clone(),
            name: format!(
                "Synthetic {} {}",
                if i < 2 { "lower" } else { "upper" },
                operation
            ),
            storey: if i < 2 { 1 } else { 2 },
            origin_m: [(i % 2) as f64 * 8., 0., (i / 2) as f64 * 3.],
            size_m: [8., 6., 3.],
            surfaces,
            windows: vec![ThermalWindow {
                id: format!("{id}_window"),
                surface_id: format!("{id}_South"),
                glazing_id: "window_glazing".into(),
                offset_u_m: 2.,
                offset_v_m: 1.,
                width_m: 3.,
                height_m: 1.5,
                overhang: if proposed {
                    Some(ThermalOverhang {
                        projection_m: 0.6,
                        side_extension_m: 0.1,
                        gap_above_m: 0.1,
                    })
                } else {
                    None
                },
            }],
            gains: ZoneGains {
                people_per_floor_m2: 0.08,
                occupancy_schedule: operation.into(),
                activity_schedule: "activity".into(),
                people_sensible_fraction: 0.6,
                people_radiant_fraction_of_sensible: 0.3,
                lighting_w_per_floor_m2: if proposed { 6. } else { 10. },
                lighting_schedule: operation.into(),
                lighting_radiant_fraction: 0.3,
                lighting_visible_fraction: 0.2,
                lighting_return_air_fraction: 0.,
                equipment_w_per_floor_m2: 8.,
                equipment_schedule: operation.into(),
                equipment_radiant_fraction: 0.3,
                equipment_latent_fraction: 0.,
                equipment_lost_fraction: 0.,
                infiltration: InfiltrationRate::AirChangesPerHour { value: 0.2 },
                infiltration_schedule: "always".into(),
                source: source(),
            },
            cooling_system: Some(DxCoolingSystem {
                id: format!("{id}_cooling"),
                gross_rated_capacity_w: 5000.,
                gross_rated_cop: if proposed { 3.6 } else { 3. },
                rated_shr: 0.75,
                rated_airflow_m3_s: 0.3,
                rated_indoor_wet_bulb_c: 19.44,
                rated_outdoor_dry_bulb_c: 35.,
                capacity_temperature_curve: "temp_modifier".into(),
                capacity_flow_curve: "flow_modifier".into(),
                eir_temperature_curve: "temp_modifier".into(),
                eir_flow_curve: "flow_modifier".into(),
                part_load_fraction_curve: "part_load".into(),
                availability_schedule: operation.into(),
                cooling_setpoint_schedule: "setpoint".into(),
                // Example revision 2: explicit load control without the optional
                // hysteretic setpoint-revision branch. Not a qualification claim.
                cooling_deadband_k: 0.,
                supply_fan: SupplyFan {
                    pressure_rise_pa: 100.,
                    total_efficiency: 0.5,
                    motor_efficiency: 0.9,
                    motor_in_air_fraction: 1.,
                },
                balanced_ventilation: BalancedVentilation {
                    design_flow_m3_s: 0.02,
                    fraction_schedule: operation.into(),
                    fan_pressure_pa: 60.,
                    total_efficiency: 0.6,
                },
                sizing_source: source(),
            }),
        });
    }
    ThermalModel {
        schema_version: THERMAL_MODEL_SCHEMA.into(),
        name: format!(
            "Original synthetic four-zone {}",
            if proposed { "proposed" } else { "baseline" }
        ),
        site: ThermalSite {
            latitude_deg_north: 0.,
            longitude_deg_east: 0.,
            elevation_m: 0.,
        },
        north_rotation_deg: 0.,
        zones,
        materials,
        constructions,
        glazing: vec![ThermalGlazing {
            id: "window_glazing".into(),
            name: "Synthetic simplified glazing".into(),
            u_value_w_m2k: if proposed { 2. } else { 3. },
            shgc: if proposed { 0.35 } else { 0.6 },
            visible_transmittance: 0.6,
            basis: GlazingBasis::WholeWindow,
            source: source(),
        }],
        curves,
        schedules,
        detached_shading: vec![],
        exterior_lighting_w: 20.,
        exterior_lighting_schedule: "always".into(),
    }
}
