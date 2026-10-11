use greenlens_core::{study::*, validate_model, Model};

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}
fn binding() -> WeatherBinding {
    WeatherBinding {
        original_sha256: "a".repeat(64),
        normalized_sha256: "b".repeat(64),
        time_axis_sha256: "c".repeat(64),
        interpretation: "test.synthetic.v1".into(),
    }
}
fn annual() -> Study {
    Study {
        calculation: StudyMode::Annual {
            weather: binding(),
            profile: NumericalProfile::Shipping,
        },
        ..Study::default()
    }
}

#[test]
fn defaults_are_3650_hours_and_fractional_or_midnight_intervals_are_explicit() {
    close(Occupancy::default().weights().unwrap().iter().sum(), 3650.0);
    let partial = Occupancy::Daily {
        start_minute: 510,
        end_minute: 1065,
    }
    .weights()
    .unwrap();
    close(partial[8], 0.5);
    close(partial[17], 0.75);
    close(partial.iter().sum(), 365.0 * 9.25);
    let wrap = Occupancy::Daily {
        start_minute: 1380,
        end_minute: 60,
    }
    .weights()
    .unwrap();
    close(wrap[23], 1.0);
    close(wrap[0], 1.0);
    close(wrap[1], 0.0);
    close(wrap.iter().sum(), 730.0);
}

#[test]
fn hand_calculated_boundaries_partial_occupancy_and_nighttime() {
    let mut weights = vec![0.0; HOURS_PER_YEAR];
    // Darkness, below-L, L, below-T, T, U, above-U, including partial occupancy.
    weights[..7].copy_from_slice(&[1.0, 0.5, 1.0, 0.5, 1.0, 1.0, 0.5]);
    let settings = MetricSettings {
        occupancy: Occupancy::Mask { weights },
        ..MetricSettings::default()
    };
    let mut a = MetricAccumulator::new(1, &settings).unwrap();
    for e in [0.0, 99.0, 100.0, 299.0, 300.0, 3000.0, 3001.0] {
        a.push_hour(&[e]).unwrap();
    }
    for _ in 7..HOURS_PER_YEAR {
        a.push_hour(&[0.0]).unwrap();
    }
    let m = a.finish().unwrap()[0];
    close(m.da_percent, 100.0 * 2.5 / 5.5);
    close(
        m.cda_percent,
        100.0 * (0.5 * 99.0 / 300.0 + 100.0 / 300.0 + 0.5 * 299.0 / 300.0 + 2.5) / 5.5,
    );
    close(m.udi_percent, 100.0 * 3.5 / 5.5);
    close(m.underlit_percent, 100.0 * 1.5 / 5.5);
    close(m.overlit_percent, 100.0 * 0.5 / 5.5);
    close(
        m.udi_percent + m.underlit_percent + m.overlit_percent,
        100.0,
    );
}

#[test]
fn empty_invalid_occupancy_and_bad_outputs_never_finalize() {
    assert!(Occupancy::Mask {
        weights: vec![0.0; HOURS_PER_YEAR]
    }
    .weights()
    .is_err());
    assert!(Occupancy::Mask {
        weights: vec![1.0; HOURS_PER_YEAR - 1]
    }
    .weights()
    .is_err());
    for bad in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(Occupancy::Mask {
            weights: vec![bad; HOURS_PER_YEAR]
        }
        .weights()
        .is_err());
    }
    assert!(Occupancy::Daily {
        start_minute: 480,
        end_minute: 480
    }
    .weights()
    .is_err());
    assert!(MetricAccumulator::new(257, &MetricSettings::default()).is_err());
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(MetricAccumulator::new(1, &MetricSettings::default())
            .unwrap()
            .push_hour(&[bad])
            .is_err());
    }
    let mut a = MetricAccumulator::new(2, &MetricSettings::default()).unwrap();
    assert!(a.push_hour(&[1.0]).is_err());
    a.push_hour(&[0.0, 0.0]).unwrap();
    assert!(a.finish().is_err());
}

#[test]
fn subnormal_occupancy_is_scale_invariant_finite_and_partitioned() {
    let mut weights = vec![0.0; HOURS_PER_YEAR];
    weights[0] = 1e-310;
    weights[1] = 1e-310;
    let settings = MetricSettings {
        occupancy: Occupancy::Mask { weights },
        ..MetricSettings::default()
    };
    let mut a = MetricAccumulator::new(1, &settings).unwrap();
    a.push_hour(&[0.0]).unwrap();
    a.push_hour(&[300.0]).unwrap();
    for _ in 2..HOURS_PER_YEAR {
        a.push_hour(&[0.0]).unwrap();
    }
    let m = a.finish().unwrap()[0];
    close(m.da_percent, 50.0);
    close(m.cda_percent, 50.0);
    close(m.udi_percent, 50.0);
    close(m.underlit_percent, 50.0);
    close(m.overlit_percent, 0.0);
    close(
        m.udi_percent + m.underlit_percent + m.overlit_percent,
        100.0,
    );
}

#[test]
fn study_identity_separates_physics_metrics_and_legacy_static() {
    let model = Model::default();
    let mut s = annual();
    let hash = s.physical_input_hash(&model).unwrap();
    let metrics = s.metrics.identity(&hash).unwrap();
    s.metrics.da_threshold_lux = 500.0;
    assert_eq!(hash, s.physical_input_hash(&model).unwrap());
    assert_ne!(metrics, s.metrics.identity(&hash).unwrap());
    let mut renamed = model.clone();
    renamed.name = "Different title".into();
    renamed.sky.exterior_horizontal_illuminance_lux = 5000.0;
    assert_eq!(hash, s.physical_input_hash(&renamed).unwrap());
    renamed.sky.ground_reflectance = 0.3;
    assert_ne!(hash, s.physical_input_hash(&renamed).unwrap());
    assert_eq!(
        Study::default().physical_input_hash(&model).unwrap(),
        validate_model(&model).unwrap().scientific_input_hash
    );
    assert_eq!(
        s.comparison_input_hash(&model).unwrap(),
        s.comparison_input_hash(&Model::alternative_b()).unwrap()
    );
    assert_ne!(
        hash,
        s.physical_input_hash(&Model::alternative_b()).unwrap()
    );
    assert!(s.metrics.identity("").is_err());
    assert!(s.metrics.identity(&"G".repeat(64)).is_err());
}

#[test]
fn weather_modes_validate_limits_versions_and_roundtrip() {
    let mut s = annual();
    let model = Model::default();
    assert_eq!(
        s,
        serde_json::from_slice::<Study>(&serde_json::to_vec(&s).unwrap()).unwrap()
    );
    s.calculation = StudyMode::WeatherHour {
        weather: binding(),
        interval_index: 8760,
        profile: NumericalProfile::Shipping,
    };
    assert!(s.validate(&model).is_err());
    s = annual();
    s.schema_version = "unknown".into();
    assert!(s.validate(&model).is_err());
    let mut dense = model;
    dense.sensors.requested_spacing_m = 0.1;
    assert!(annual().validate(&dense).is_err());
    assert!(Study::default().validate(&dense).is_ok());
}
