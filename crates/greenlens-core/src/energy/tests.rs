use super::*;
use std::collections::BTreeMap;

fn report_fixture() -> EnergyReportData {
    static FIXTURE: std::sync::OnceLock<EnergyReportData> = std::sync::OnceLock::new();
    FIXTURE
        .get_or_init(|| synthetic_energy_report().unwrap())
        .clone()
}
#[test]
fn example_revision_two_changes_only_explicit_cutout_from_retained_revision_one() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../examples/energy/revision1-inputs.json"
    ))
    .unwrap();
    let weather = weather::parse_energy_epw(&weather::synthetic_energy_epw())
        .unwrap()
        .data;
    for (alternative, proposed) in [("A", false), ("B", true)] {
        let original: ThermalModel =
            serde_json::from_value(fixture["cases"][alternative]["model"].clone()).unwrap();
        let original_study: EnergyStudy =
            serde_json::from_value(fixture["cases"][alternative]["study"].clone()).unwrap();
        assert_eq!(original_study.calendar, synthetic_energy_calendar());
        assert_eq!(original_study.weather, weather.binding());
        assert_eq!(original_study.zone_timesteps_per_hour, 6);
        let revised = synthetic_four_zone_model_revision_two(proposed);
        assert_eq!(revised.zones.len(), 4);
        assert!(original.zones.iter().all(|zone| zone
            .cooling_system
            .as_ref()
            .unwrap()
            .cooling_deadband_k
            == 0.2));
        assert!(revised.zones.iter().all(|zone| zone
            .cooling_system
            .as_ref()
            .unwrap()
            .cooling_deadband_k
            == 0.));
        let mut restored = revised.clone();
        for zone in &mut restored.zones {
            zone.cooling_system.as_mut().unwrap().cooling_deadband_k = 0.2;
        }
        // Compares every other typed leaf against immutable native-run inputs,
        // including schedules, curves, geometry, gains, source and equipment.
        assert_eq!(restored, original);
        let old = validate_thermal_model(&original, &original_study.calendar).unwrap();
        let new = validate_thermal_model(&revised, &original_study.calendar).unwrap();
        assert_ne!(old.model_input_sha256, new.model_input_sha256);
        validate_weather_context(&revised, &original_study, &weather).unwrap();
    }
}
#[test]
fn example_revision_three_changes_only_four_proposed_fan_efficiencies() {
    let baseline = synthetic_four_zone_model(false);
    assert_eq!(baseline, synthetic_four_zone_model_revision_two(false));
    let previous = synthetic_four_zone_model_revision_two(true);
    let proposed = synthetic_four_zone_model(true);
    assert_eq!(previous.zones.len(), 4);
    assert_eq!(proposed.zones.len(), 4);
    let mut restored = proposed.clone();
    for ((old, new), restore) in previous
        .zones
        .iter()
        .zip(&proposed.zones)
        .zip(&mut restored.zones)
    {
        let old_system = old.cooling_system.as_ref().unwrap();
        let new_system = new.cooling_system.as_ref().unwrap();
        assert_eq!(old_system.supply_fan.total_efficiency, 0.5);
        assert_eq!(new_system.supply_fan.total_efficiency, 0.6);
        close(
            old_system.rated_airflow_m3_s * old_system.supply_fan.pressure_rise_pa
                / old_system.supply_fan.total_efficiency,
            60.,
        );
        close(
            new_system.rated_airflow_m3_s * new_system.supply_fan.pressure_rise_pa
                / new_system.supply_fan.total_efficiency,
            50.,
        );
        restore
            .cooling_system
            .as_mut()
            .unwrap()
            .supply_fan
            .total_efficiency = 0.5;
    }
    // Complete typed equality rejects every additional edit, including geometry,
    // schedules, airflow, ventilation, motor heat, curves, COP and cutout.
    assert_eq!(restored, previous);
    let calendar = synthetic_energy_calendar();
    let old = validate_thermal_model(&previous, &calendar).unwrap();
    let new = validate_thermal_model(&proposed, &calendar).unwrap();
    assert_ne!(old.model_input_sha256, new.model_input_sha256);
    assert_eq!(
        validate_thermal_model(&baseline, &calendar)
            .unwrap()
            .model_input_sha256,
        validate_thermal_model(&synthetic_four_zone_model_revision_two(false), &calendar)
            .unwrap()
            .model_input_sha256
    );
}
#[test]
fn report_snapshot_presentation_preserves_values_and_missing_states() {
    let report = report_fixture();
    let presentation = report.presentation().unwrap();
    close(report.baseline.totals.annual.facility_kwh, 8760.);
    close(report.proposed.totals.annual.facility_kwh, 7008.);
    assert!(matches!(
        presentation.baseline.emissions,
        ReportCell::Missing { .. }
    ));
    if let ReportCell::Value { value, display, .. } = presentation.facility_energy_savings.percent {
        close(value, 20.);
        assert_eq!(display, "20.00");
    } else {
        panic!()
    }
    assert_eq!(
        report.baseline.assessment.execution,
        EnergyExecutionStatus::NotExecuted
    );
    assert_eq!(
        report.baseline.assessment.scientific.status,
        EnergyQualificationStatus::Unassessed
    );
    assert_eq!(
        serde_json::from_str::<EnergyReportData>(&serde_json::to_string(&report).unwrap()).unwrap(),
        report
    );
}
#[test]
fn report_rejects_other_model_asset_snapshot_and_divergent_comparison() {
    let original = report_fixture();
    let mut report = original.clone();
    report.baseline.model.zones[0].gains.lighting_w_per_floor_m2 += 1.;
    assert!(report.validate().is_err());
    let mut report = original.clone();
    report.assets.push(EnergyReportAsset {
        id: "plan".into(),
        relative_path: "assets/plan.svg".into(),
        sha256: "0".repeat(64),
        caption: "Plan".into(),
        alt_text: "Plan".into(),
        model_input_sha256: "f".repeat(64),
    });
    assert!(report.validate().is_err());
    let mut report = original.clone();
    report.comparison.facility_energy_savings.absolute += 1.;
    assert!(report.validate().is_err());
    let mut report = original;
    report.comparison.input_relation.unchanged_service_inputs = false;
    assert!(report.validate().is_err());
}
#[test]
fn report_rejects_inconsistent_monthly_components_and_comfort_denominators() {
    let mut report = report_fixture();
    *report.baseline.totals.monthly[0]
        .end_uses_kwh
        .get_mut(&EnergyEndUse::CoolingSystem)
        .unwrap() += 1.;
    assert!(report.validate().is_err());
    let mut report = report_fixture();
    report.baseline.totals.building_comfort_union.unmet_percent = None;
    assert!(report.validate().is_err());
    let mut report = report_fixture();
    report.baseline.totals.coincident_peak.facility_kw = f64::INFINITY;
    assert!(report.validate().is_err());
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9_f64.max(b.abs() * 1e-12), "{a} != {b}");
}
fn ledger() -> MeterLedger {
    MeterLedger {
        version: "synthetic.v1".into(),
        facility_native_channel: "facility-electricity".into(),
        entries: [EnergyEndUse::CoolingSystem, EnergyEndUse::InteriorLighting]
            .into_iter()
            .map(|u| MeterLedgerEntry {
                end_use: u,
                fuel: Fuel::Electricity,
                native_channels: vec![format!("{u:?}-electricity")],
                boundary: "Synthetic disjoint electrical component, not delivered thermal load"
                    .into(),
                classification: ProcessClassification::Unclassified,
                classification_source: None,
            })
            .collect(),
    }
}
fn row(index: usize, a: f64, b: f64) -> EnergyInterval {
    EnergyInterval {
        index,
        facility_j: a + b,
        end_uses_j: BTreeMap::from([
            (EnergyEndUse::CoolingSystem, a),
            (EnergyEndUse::InteriorLighting, b),
        ]),
        comfort: vec![],
    }
}
fn study() -> EnergyStudy {
    EnergyStudy {
        schema_version: ENERGY_STUDY_SCHEMA.into(),
        weather: EnergyWeatherBinding {
            original_sha256: "1".repeat(64),
            normalized_sha256: "2".repeat(64),
            time_axis_sha256: "3".repeat(64),
            profile_id: weather::ENERGY_EPW_PROFILE.into(),
        },
        calendar: synthetic_energy_calendar(),
        zone_timesteps_per_hour: 6,
    }
}
#[test]
fn four_zone_topology_areas_u_and_facade_values_are_independent() {
    for proposed in [false, true] {
        let model = synthetic_four_zone_model(proposed);
        let v = validate_thermal_model(&model, &synthetic_energy_calendar()).unwrap();
        close(v.gross_floor_area_m2, 192.);
        close(v.conditioned_floor_area_m2, 192.);
        close(
            v.construction_u_values_w_m2k["external"],
            1. / (0.13 + 0.04 + 0.2 / 0.8 + if proposed { 0.08 / 0.04 } else { 0.04 / 0.04 }),
        );
        assert_eq!(v.surfaces.len(), 24);
        let south = v
            .facade_wwr
            .iter()
            .find(|f| f.orientation == "south")
            .unwrap();
        close(south.gross_wall_m2, 96.);
        close(south.opening_m2, 18.);
        close(south.ratio.unwrap(), 0.1875);
        for s in &v.surfaces {
            let a = s.vertices_m[0];
            let b = s.vertices_m[1];
            let c = s.vertices_m[2];
            let u = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
            let w = std::array::from_fn::<_, 3, _>(|i| c[i] - a[i]);
            let cross = [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ];
            assert!(
                cross
                    .iter()
                    .zip(s.outward_normal)
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
                    > 0.
            );
        }
    }
}
#[test]
fn topology_rejects_overlap_partial_face_and_unpaired_adjacency() {
    let original = synthetic_four_zone_model(false);
    let c = synthetic_energy_calendar();
    let mut m = original.clone();
    m.zones[1].origin_m[0] = 7.9;
    assert!(validate_thermal_model(&m, &c).is_err());
    let mut m = original.clone();
    m.zones[1].origin_m[1] = 0.5;
    assert!(validate_thermal_model(&m, &c).is_err());
    let mut m = original.clone();
    m.zones[0]
        .surfaces
        .iter_mut()
        .find(|s| s.face == ThermalFace::East)
        .unwrap()
        .boundary = ThermalBoundary::Outdoors;
    assert!(validate_thermal_model(&m, &c).is_err());
    let mut m = original;
    m.zones[2].origin_m[2] = 3.1;
    assert!(validate_thermal_model(&m, &c).is_err());
}
#[test]
fn opening_overlap_bad_layers_and_unsupported_zone_count_fail() {
    let original = synthetic_four_zone_model(false);
    let c = synthetic_energy_calendar();
    let mut m = original.clone();
    let mut w = m.zones[0].windows[0].clone();
    w.id = "overlap".into();
    m.zones[0].windows.push(w);
    assert!(validate_thermal_model(&m, &c).is_err());
    let mut m = original.clone();
    m.zones[0].windows[0].width_m = 9.;
    assert!(validate_thermal_model(&m, &c).is_err());
    let mut m = original.clone();
    m.constructions[1].layers = vec!["masonry".into(), "insulation".into()];
    assert!(validate_thermal_model(&m, &c).is_err());
    let mut m = original;
    m.zones = vec![m.zones[0].clone(); 9];
    assert!(validate_thermal_model(&m, &c).is_err());
}
#[test]
fn global_rotation_changes_vectors_without_area_or_wwr_loss() {
    let mut m = synthetic_four_zone_model(false);
    m.north_rotation_deg = 90.;
    let v = validate_thermal_model(&m, &synthetic_energy_calendar()).unwrap();
    let east = v.surfaces.iter().find(|s| s.id == "zone2_East").unwrap();
    close(east.outward_normal[0], 0.);
    close(east.outward_normal[1], -1.);
    close(v.facade_wwr.iter().map(|f| f.opening_m2).sum(), 18.);
}
#[test]
fn schedule_calendar_expansion_holidays_weekends_and_last_boundary() {
    let c = synthetic_energy_calendar();
    let mut s = synthetic_four_zone_model(false).schedules.remove(0);
    s.weekday = DayProfile::constant(1.);
    s.saturday = DayProfile::constant(0.5);
    s.sunday = DayProfile::constant(0.25);
    let x = expand_schedule(&s, &c).unwrap();
    assert_eq!(x.len(), 52560);
    close(x[0], 0.);
    close(x[144], 1.);
    close(x[5 * 144], 0.5);
    close(x[6 * 144], 0.25);
    let last = interval_timestamp(52559).unwrap();
    assert_eq!(
        (last.month, last.day, last.start_minute, last.end_minute),
        (12, 31, 1430, 1440)
    );
    assert!(interval_timestamp(52560).is_err());
    s.holidays.clear();
    assert!(expand_schedule(&s, &c).is_err());
    assert!(MonthDay { month: 2, day: 29 }.day_index().is_err());
    let mut c = c;
    c.holidays.push(c.holidays[0]);
    assert!(c.validate().is_err());
}
#[test]
fn physical_identity_tracks_schedules_not_display_name_and_carbon_is_separate() {
    let a = synthetic_four_zone_model(false);
    let s = study();
    let hash = s.physical_input_hash(&a).unwrap();
    let mut b = a.clone();
    b.name = "Renamed".into();
    b.zones[0].name = "Renamed zone".into();
    assert_eq!(hash, s.physical_input_hash(&b).unwrap());
    b.schedules[0].weekday.values[50] = 0.5;
    assert_ne!(hash, s.physical_input_hash(&b).unwrap());
    let p = processing_identity(&"4".repeat(64), &ledger()).unwrap();
    let mut f = CarbonFactor {
        id: "synthetic".into(),
        fuel: Fuel::Electricity,
        region: "Artificial".into(),
        year: 2025,
        kg_per_kwh: 0.5,
        unit: EmissionUnit::KgCo2e,
        source: "Original synthetic factor".into(),
        version: "1".into(),
        synthetic: true,
    };
    let old = carbon_identity(&p, &f).unwrap();
    f.kg_per_kwh = 0.4;
    assert_ne!(old, carbon_identity(&p, &f).unwrap());
    assert_eq!(hash, s.physical_input_hash(&a).unwrap());
}
#[test]
fn system_parameters_and_service_changes_are_not_hidden() {
    let a = synthetic_four_zone_model(false);
    let s = study();
    let mut b = synthetic_four_zone_model(true);
    let relation = compare_energy_inputs(&a, &s, &b, &s).unwrap();
    assert!(relation.comparable_geometry_weather_calendar);
    assert!(relation.unchanged_service_inputs);
    b.schedules[4].weekday.values[60] = 26.;
    assert!(
        !compare_energy_inputs(&a, &s, &b, &s)
            .unwrap()
            .unchanged_service_inputs
    );
    b.zones[0].cooling_system.as_mut().unwrap().gross_rated_cop = f64::NAN;
    assert!(s.validate(&b).is_err());
}
#[test]
fn boundary_curve_and_dedicated_ventilation_schedule_changes_are_disclosed() {
    let a = synthetic_four_zone_model(false);
    let mut b = a.clone();
    let s = study();
    b.zones[0]
        .surfaces
        .iter_mut()
        .find(|x| x.face == ThermalFace::North)
        .unwrap()
        .boundary = ThermalBoundary::Adiabatic {
        reason: "Explicit changed test boundary".into(),
    };
    assert!(compare_energy_inputs(&a, &s, &b, &s)
        .unwrap()
        .differences
        .iter()
        .any(|x| x.contains("boundary")));
    let mut a = a;
    let mut vent = a.schedules[2].clone();
    vent.id = "independent_vent".into();
    a.schedules.push(vent);
    a.zones[0]
        .cooling_system
        .as_mut()
        .unwrap()
        .balanced_ventilation
        .fraction_schedule = "independent_vent".into();
    let mut b = a.clone();
    b.schedules.last_mut().unwrap().weekday.values[0] = 0.;
    assert!(
        !compare_energy_inputs(&a, &s, &b, &s)
            .unwrap()
            .unchanged_service_inputs
    );
}
#[test]
fn explicit_thermal_weather_rejects_engine_fallbacks_and_location_override() {
    let bytes = weather::synthetic_energy_epw();
    let parsed = weather::parse_energy_epw(&bytes).unwrap();
    assert_eq!(parsed.data.hours.len(), 8760);
    let mut s = study();
    s.weather = parsed.data.binding();
    let mut m = synthetic_four_zone_model(false);
    m.site = ThermalSite {
        latitude_deg_north: parsed.data.location.latitude_deg_north,
        longitude_deg_east: parsed.data.location.longitude_deg_east,
        elevation_m: parsed.data.location.elevation_m,
    };
    s.calendar.utc_offset_minutes = parsed.data.location.utc_offset_minutes;
    validate_weather_context(&m, &s, &parsed.data).unwrap();
    s.calendar.utc_offset_minutes += 60;
    assert!(validate_weather_context(&m, &s, &parsed.data).is_err());
    let original = String::from_utf8(bytes).unwrap();
    for (field, value) in [
        (9, "31000"),
        (12, "9999"),
        (21, "999"),
        (30, "999"),
        (32, "999"),
        (33, "999"),
        (8, "101"),
    ] {
        let mut lines: Vec<_> = original.lines().map(str::to_owned).collect();
        let mut fields: Vec<_> = lines[8].split(',').map(str::to_owned).collect();
        fields[field] = value.into();
        lines[8] = fields.join(",");
        assert!(
            weather::parse_energy_epw((lines.join("\n") + "\n").as_bytes()).is_err(),
            "field{field}"
        );
    }
}
#[test]
fn conversion_savings_and_missing_denominators_are_explicit() {
    close(joules_to_kwh(3_600_000.).unwrap(), 1.);
    close(interval_demand_kw(600_000., 600).unwrap(), 1.);
    assert_eq!(savings(0., 0.).unwrap().percent, None);
    assert_eq!(savings(0., 3.).unwrap().absolute, -3.);
    close(savings(10., 12.).unwrap().percent.unwrap(), -20.);
    assert!(savings(f64::NAN, 1.).is_err());
    assert!(interval_demand_kw(1., 0).is_err());
    let c = ReportCell::value(1.23456789, EnergyUnit::Kwh, 2).unwrap();
    if let ReportCell::Value { value, display, .. } = c {
        assert_eq!(value, 1.23456789);
        assert_eq!(display, "1.23");
    } else {
        panic!()
    }
}
#[test]
fn ledger_rejects_facility_and_duplicate_channel_double_count() {
    let mut l = ledger();
    l.entries[1].native_channels = l.entries[0].native_channels.clone();
    assert!(l.validate().is_err());
    let mut l = ledger();
    l.entries[0].native_channels = vec![l.facility_native_channel.clone()];
    assert!(l.validate().is_err());
    let mut l = ledger();
    l.entries[0].classification = ProcessClassification::Process;
    assert!(l.validate().is_err());
}
#[test]
fn complete_constant_year_reconciles_months_and_peak_is_coincident() {
    let mut a = EnergyAccumulator::new(&ledger(), &[]).unwrap();
    for i in 0..52560 {
        let (x, y) = match i {
            0 => (1_200_000., 0.),
            1 => (0., 1_800_000.),
            _ => (300_000., 300_000.),
        };
        a.push(&row(i, x, y)).unwrap();
    }
    let t = a.finish().unwrap();
    close(t.annual.facility_kwh, 8760. + 0.5);
    close(t.monthly[0].facility_kwh, 744. + 0.5);
    close(t.coincident_peak.facility_kw, 3.);
    assert_eq!(t.coincident_peak.timestamp.interval_index, 1);
    close(
        t.coincident_peak.end_use_contributions_kw[&EnergyEndUse::CoolingSystem],
        0.,
    );
    close(
        t.coincident_peak.end_use_contributions_kw[&EnergyEndUse::InteriorLighting],
        3.,
    );
    assert_eq!(t.building_comfort_union.unmet_percent, None);
}
#[test]
fn five_minute_demand_pairs_before_peak_but_comfort_retains_excursions() {
    let mut a =
        EnergyAccumulator::with_interval_seconds(&ledger(), &["z1".into(), "z2".into()], 300)
            .unwrap();
    for i in 0..105120 {
        let mut r = row(i, if i % 2 == 0 { 1_800_000. } else { 0. }, 0.);
        r.comfort = vec![
            ComfortSample {
                zone_id: "z1".into(),
                occupied: i < 2,
                air_temperature_c: if i == 0 { 24.3 } else { 24.2 },
                cooling_setpoint_c: 24.,
            },
            ComfortSample {
                zone_id: "z2".into(),
                occupied: i < 2,
                air_temperature_c: if i == 1 { 24.3 } else { 24.2 },
                cooling_setpoint_c: 24.,
            },
        ];
        a.push(&r).unwrap();
    }
    let t = a.finish().unwrap();
    assert_eq!(
        (
            t.interval_count,
            t.interval_seconds,
            t.demand_interval_seconds
        ),
        (105120, 300, 600)
    );
    close(t.coincident_peak.facility_kw, 3.);
    close(t.building_comfort_union.occupied_hours, 1. / 6.);
    close(t.building_comfort_union.cooling_unmet_hours, 1. / 6.);
    close(t.zone_comfort[0].cooling_unmet_hours, 1. / 12.);
    close(t.building_comfort_union.unmet_percent.unwrap(), 100.);
}
#[test]
fn malformed_rows_do_not_consume_index_or_finish_partial_year() {
    let mut a = EnergyAccumulator::new(&ledger(), &[]).unwrap();
    let mut r = row(0, 1., 1.);
    r.facility_j = 600_000.;
    assert!(a.push(&r).is_err());
    a.push(&row(0, 1., 1.)).unwrap();
    assert!(a.push(&row(0, 1., 1.)).is_err());
    assert!(a.finish().is_err());
    let mut a = EnergyAccumulator::new(&ledger(), &[]).unwrap();
    a.push(&row(0, 1e308, 0.)).unwrap();
    assert!(a.push(&row(1, 1e308, 0.)).is_err());
    a.push(&row(1, 0., 0.)).unwrap();
}
#[test]
fn four_axis_qualification_does_not_inherit_daylight_or_other_inputs() {
    let h = "0".repeat(64);
    let method = EnergyMethodIdentity {
        method_id: "energy-unassessed".into(),
        adapter_version: "1".into(),
        engine_version: "26.2.0".into(),
        engine_package_sha256: h.clone(),
        engine_executable_sha256: h.clone(),
        engine_schema_sha256: h.clone(),
        recipe_sha256: h.clone(),
        numerical_settings_sha256: h.clone(),
    };
    let mut a = EnergyAssessment {
        execution: EnergyExecutionStatus::Completed,
        evidence_integrity: EnergyIntegrityScope::CompleteRetainedEvidenceChecked,
        current_physical_inputs: EnergyInputRelation::Current,
        current_processed_results: EnergyInputRelation::Current,
        current_derived_results: EnergyInputRelation::Current,
        scientific: EnergyQualification {
            status: EnergyQualificationStatus::Unassessed,
            method: method.clone(),
            registry_version: "none".into(),
            reason: "No applicable energy assessment".into(),
            tested_scope: "No inherited daylight scope".into(),
            applicable_physical_input_sha256: vec![],
            evidence_ids: vec![],
        },
    };
    a.validate(&h, &method).unwrap();
    a.scientific.status = EnergyQualificationStatus::LimitedQualified;
    assert!(a.validate(&h, &method).is_err());
    a.scientific.applicable_physical_input_sha256 = vec![h.clone()];
    a.scientific.evidence_ids = vec!["limited-original-fixture".into()];
    a.validate(&h, &method).unwrap();
    assert!(a.validate(&"1".repeat(64), &method).is_err());
}
