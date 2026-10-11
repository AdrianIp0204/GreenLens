//! Synthetic persistence fixtures only. No engine is executed, and these are
//! never annual transport, real-weather or native acceptance evidence.
use super::*;
use crate::{
    persistence::{seal, write_json},
    timeseries::SeriesWriter,
    weather::WeatherSource,
};
use std::fs;
use tempfile::tempdir;

fn model() -> Model {
    let mut model = Model::default();
    model.sensors.requested_spacing_m = 4.0;
    model
}
fn source() -> WeatherSource {
    WeatherSource {
        citation: "Original synthetic fixture; no climate or process observations".into(),
        source_url: None,
        license: "Apache-2.0".into(),
        rights_note: "Synthetic data authored for GreenLens tests".into(),
        synthetic_non_climatic: true,
    }
}
fn engine(method: &str) -> annual::AnnualEngineIdentity {
    let adapter = match method {
        annual::LEGACY_METHOD_VERSION => annual::LEGACY_ADAPTER_VERSION,
        annual::EXPLICIT_METHOD_VERSION => annual::EXPLICIT_ADAPTER_VERSION,
        annual::METHOD_VERSION => annual::ADAPTER_VERSION,
        _ => panic!("Unknown synthetic method"),
    };
    let mut engine = annual::AnnualEngineIdentity {
        version: greenlens_radiance::ENGINE_VERSION.into(),
        adapter_version: adapter.into(),
        method_version: method.into(),
        package_sha256: "c4f02efcb4a33f4a43b7ce0ba58100544abc317a1be61f7b0425a64a66eb8a09".into(),
        executable_sha256: [
            (
                "rtrace.exe",
                "edb01ac56b31ad79263c1f42a686413c0d369829f460e83a170340a6f821b5c9",
            ),
            (
                "gendaymtx.exe",
                "07a022f4569c7815bba868148add3706aa0e120622f0e8e7c7029e7947af527d",
            ),
            (
                "rcontrib.exe",
                "0e622eb168ce5ba30bfad2d154bf64bc4555d1663f1d20c9c1a833f947816975",
            ),
            (
                "dctimestep.exe",
                "ccd4ee7b89c73f752419642dc10f93e13b6c75b407f0e12775f5056ec7f1ab05",
            ),
            (
                "oconv.exe",
                "e0bc7240ee2645d6590c33ef5a6a783f4bfe81246dce23bfefdc66966cba60a2",
            ),
        ]
        .map(|(name, value)| (name.into(), value.into()))
        .into(),
        library_sha256: [
            (
                "lib/rayinit.cal",
                "293b321513e9fedce90ad6e88f2ef00d6e9184d0581e8fdeb111fd05fa32ae60",
            ),
            (
                "lib/skybright.cal",
                "323d4762faa32b6a0ee086d97df6b04ba39416385af927247265de6d428fe454",
            ),
            (
                "lib/reinhart.cal",
                "1517d597e34673bd8c5cfcea79850520ac272c7f5229df22401537f0b7c82934",
            ),
        ]
        .map(|(name, value)| (name.into(), value.into()))
        .into(),
        fingerprint: String::new(),
    };
    engine.fingerprint = canonical_hash(&engine).unwrap();
    engine
}
fn identities(record: &mut ClimateRunRecord) {
    record.provenance["physical_input_hash"] =
        json!(record.study.physical_input_hash(&record.model).unwrap());
    record.provenance["run_fingerprint"] = json!(run_fingerprint(record).unwrap());
    record.provenance["comparison_fingerprint"] = json!(comparison_fingerprint(record).unwrap());
    record.provenance["physical_result_fingerprint"] =
        json!(physical_result_fingerprint(record).unwrap());
    record.provenance["metric_identity"] = if record.metrics.is_some() {
        json!(record
            .study
            .metrics
            .identity(&physical_result_fingerprint(record).unwrap())
            .unwrap())
    } else {
        Value::Null
    };
}
fn save_record(root: &Path, record: &ClimateRunRecord) {
    write_json(&root.join("results.json"), record).unwrap();
    write_json(&root.join("input.json"), &record.model).unwrap();
    write_json(&root.join("study.json"), &record.study).unwrap();
    write_json(&root.join("provenance.json"), &record.provenance).unwrap();
    reseal(root, record);
}
fn reseal(root: &Path, record: &ClimateRunRecord) {
    let files = evidence_files(record);
    seal(root, &files.iter().map(String::as_str).collect::<Vec<_>>()).unwrap();
}
pub(crate) fn fixture(root: &Path, annual_result: bool) -> ClimateRunRecord {
    fixture_at(root, annual_result, 0)
}
pub(crate) fn fixture_for_method(
    root: &Path,
    annual_result: bool,
    method: &str,
) -> ClimateRunRecord {
    fixture_at_method(root, annual_result, 0, method)
}
fn fixture_at(root: &Path, annual_result: bool, selected_interval: usize) -> ClimateRunRecord {
    fixture_at_method(
        root,
        annual_result,
        selected_interval,
        annual::METHOD_VERSION,
    )
}
fn fixture_at_method(
    root: &Path,
    annual_result: bool,
    selected_interval: usize,
    method: &str,
) -> ClimateRunRecord {
    fs::create_dir_all(root).unwrap();
    let temporary = tempdir().unwrap();
    let original = temporary.path().join("synthetic.epw");
    fs::write(&original, greenlens_core::weather::synthetic_epw()).unwrap();
    weather::import(
        &original,
        &root.join("weather"),
        greenlens_core::weather::WeatherSourceProfile::EpwHourEnding,
        source(),
    )
    .unwrap();
    let (asset, parsed) = weather::open(&root.join("weather")).unwrap();
    let model = model();
    let validated = validate_model(&model).unwrap();
    let study = Study {
        calculation: if annual_result {
            StudyMode::Annual {
                weather: asset.binding.clone(),
                profile: NumericalProfile::Shipping,
            }
        } else {
            StudyMode::WeatherHour {
                weather: asset.binding.clone(),
                interval_index: selected_interval,
                profile: NumericalProfile::Shipping,
            }
        },
        ..Study::default()
    };
    let mut writer = SeriesWriter::create(
        &root.join("series"),
        validated
            .sensor_grid
            .sensors
            .iter()
            .map(|s| s.id.clone())
            .collect(),
        if annual_result { HOURS_PER_YEAR } else { 1 },
    )
    .unwrap();
    for _ in 0..if annual_result { HOURS_PER_YEAR } else { 1 } {
        writer
            .push_hour(&vec![[0.0; 3]; validated.sensor_grid.sensors.len()])
            .unwrap();
    }
    let series = writer.finish().unwrap();
    let metrics = annual_result.then(|| {
        SeriesReader::open(&root.join("series"))
            .unwrap()
            .metrics(&study.metrics)
            .unwrap()
    });
    let identity = engine(method);
    let profile = annual::AnnualProfile::shipping_for_method(method).unwrap();
    let mut record = ClimateRunRecord {
        schema_version: RUN_SCHEMA.into(),
        id: Uuid::new_v4().to_string(),
        alternative_id: "A".into(),
        model,
        validated,
        study,
        weather_asset: asset,
        series,
        metrics,
        provenance: json!({"schema_version":RUN_SCHEMA,"application_version":"synthetic-test","adapter_version":identity.adapter_version,
            "test_fixture":"MOCK: persistence only; no process was executed","calculation_label":if annual_result {ANNUAL_LABEL} else {HOUR_LABEL},
            "state":"completed","units":"lx","engine":identity,"profile":profile,
            "child_environment":{"RAYPATH":".;<compatible-engine>/lib"},
            "started_utc":"2026-01-01T00:00:00Z","finished_utc":"2026-01-01T00:00:00Z"}),
    };
    record.provenance["input_hash"] = json!(record.validated.normalized_input_hash);
    record.provenance["sensor_hash"] = json!(record.validated.sensor_hash);
    record.provenance["time_axis_sha256"] = json!(record.weather_asset.binding.time_axis_sha256);
    let scene = greenlens_radiance::scene_text(&record.validated).unwrap();
    let sensors = record
        .validated
        .sensor_grid
        .sensors
        .iter()
        .map(|s| {
            format!(
                "{} {} {} {} {} {}\n",
                s.position[0],
                s.position[1],
                s.position[2],
                s.direction[0],
                s.direction[1],
                s.direction[2]
            )
        })
        .collect::<String>();
    let (generated, context) = annual::weather_window(
        &parsed.normalized_epw,
        if annual_result {
            None
        } else {
            Some(selected_interval)
        },
    )
    .unwrap();
    let mut recipe = annual::AnnualRecipe {
        schema_version: recipe_contract(method).unwrap().0.into(),
        method_version: method.into(),
        engine: identity,
        profile: profile.clone(),
        normalized_weather_sha256: record.weather_asset.binding.normalized_sha256.clone(),
        generated_weather_sha256: bytes_hash(&generated),
        sensor_sha256: bytes_hash(sensors.as_bytes()),
        scene_sha256: bytes_hash(scene.as_bytes()),
        sensor_count: record.series.sensor_ids.len(),
        interval_indices: if annual_result {
            (0..HOURS_PER_YEAR).collect()
        } else {
            vec![selected_interval]
        },
        generated_weather_interval_indices: context,
        solar_interval_indices: vec![],
        sky_rows: 144 * profile.sky_basis_mf * profile.sky_basis_mf + 2,
        ordering: annual::RESULT_ORDERING.into(),
        units: annual::RESULT_UNITS.into(),
        time_policy: annual::TIME_POLICY.into(),
        ground_policy: recipe_contract(method).unwrap().1.into(),
        direct_policy: recipe_contract(method).unwrap().2.into(),
        direct_batches: vec![],
        child_environment: BTreeMap::from([("RAYPATH".into(), ".;<engine>/lib".into())]),
        processes: vec![],
    };
    recipe.processes = expected_processes(&record, &recipe)
        .unwrap()
        .into_iter()
        .map(|(executable, arguments)| ProcessRecord {
            executable,
            arguments,
            exit_code: 0,
            started_utc: "2026-01-01T00:00:00Z".into(),
            finished_utc: "2026-01-01T00:00:00Z".into(),
            elapsed_ms: 0,
            stderr: "MOCK: no process executed".into(),
        })
        .collect();
    record.provenance["processes"] = serde_json::to_value(&recipe.processes).unwrap();
    identities(&mut record);
    fs::write(root.join("annual-scene.rad"), scene).unwrap();
    fs::write(root.join("annual-sensors.pts"), &sensors).unwrap();
    fs::write(root.join("annual-weather.epw"), generated).unwrap();
    fs::write(root.join("annual-receiver.rad"), annual::RECEIVER_TEXT).unwrap();
    if method == annual::METHOD_VERSION {
        fs::write(
            root.join("annual-coefficient-receiver.rad"),
            profile.coefficient_receiver_text(),
        )
        .unwrap();
        fs::write(
            root.join("annual-coefficient-sensors.pts"),
            profile.coefficient_sensor_text(&sensors).unwrap(),
        )
        .unwrap();
    }
    fs::write(root.join("annual-suns.rad"), "").unwrap();
    fs::write(root.join("annual-sun-modifiers.txt"), "").unwrap();
    record.provenance["solar_sources_sha256"] = json!(bytes_hash(b""));
    record.provenance["solar_modifiers_sha256"] = json!(bytes_hash(b""));
    write_json(&root.join("annual-recipe.json"), &recipe).unwrap();
    record.provenance["recipe_sha256"] = json!(bytes_hash(
        &fs::read(root.join("annual-recipe.json")).unwrap()
    ));
    write_json(&root.join("annual-logs.json"), &recipe.processes).unwrap();
    write_json(&root.join("annual-resources.json"), &json!({
        "schema_version":"greenlens.annual-resources.v1", "measurement_policy":annual::RESOURCE_MEASUREMENT_POLICY,
        "started_utc":"2026-01-01T00:00:00Z", "finished_utc":"2026-01-01T00:00:00Z", "total_wall_ms":0,
        "peak_engine_working_set_bytes":0,"peak_observed_runner_plus_engine_working_set_bytes":0,"peak_scratch_bytes":0,
        "max_scratch_bytes":2_u64*1024*1024*1024,"max_matrix_bytes":512_u64*1024*1024,"max_observed_working_set_bytes":2_u64*1024*1024*1024,
        "processes":recipe.processes.iter().map(|p| json!({"executable":p.executable,"started_utc":p.started_utc,"finished_utc":p.finished_utc,"elapsed_ms":0,
            "peak_child_working_set_bytes":0,"peak_observed_runner_plus_child_working_set_bytes":0,"peak_scratch_bytes":0,"output_bytes":0,"samples":1})).collect::<Vec<_>>()
    })).unwrap();
    save_record(root, &record);
    record
}

#[test]
fn selected_hour_copy_retains_weather_and_results_without_original_paths() {
    let temp = tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    let record = fixture(&scratch, false);
    assert!(read_run_directory(&scratch).is_ok());
    let source = temp.path().join("source");
    let copy = temp.path().join("copy");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&copy).unwrap();
    let weather_ref = import_project_weather(
        &scratch.join("weather"),
        &source,
        &Uuid::new_v4().to_string(),
    )
    .unwrap();
    let mut extension = ClimateProject::default();
    extension.weather.push(weather_ref);
    extension.studies.insert("A".into(), record.study.clone());
    append_run(&mut extension, &record).unwrap();
    append_run(&mut extension, &record).unwrap();
    assert_eq!(extension.runs.len(), 1);
    promote_run(&scratch, &source, &record.id).unwrap();
    let alternatives = vec![
        Alternative {
            id: "A".into(),
            model: model(),
        },
        Alternative {
            id: "B".into(),
            model: Model::alternative_b(),
        },
    ];
    validate_project_extension(&extension, &alternatives).unwrap();
    copy_project_evidence(&source, &copy, &extension).unwrap();
    fs::rename(&scratch, temp.path().join("scratch-unavailable")).unwrap();
    fs::rename(&source, temp.path().join("source-unavailable")).unwrap();
    verify_project_evidence(&copy, &extension).unwrap();
    let restored = read_run(&copy, &extension.runs[0]).unwrap();
    assert_eq!(restored.provenance, record.provenance);
    assert_eq!(restored.series, record.series);
    extension.weather.clear();
    // Historical snapshots remain independent of current project weather assets.
    extension.studies.insert("A".into(), Study::default());
    verify_project_evidence(&copy, &extension).unwrap();
}

#[test]
fn resealed_derived_metadata_raw_chunks_and_unsupported_recipe_are_rejected() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("run");
    let record = fixture(&root, false);
    let mut changed = record.clone();
    changed.series.sensor_ids.reverse();
    identities(&mut changed);
    assert!(validate_record(&changed).is_err());
    changed = record.clone();
    changed.provenance["time_axis_sha256"] = json!("1".repeat(64));
    assert!(validate_record(&changed).is_err());
    changed = record.clone();
    changed.provenance["application_version"] = Value::Null;
    assert!(validate_record(&changed).is_err());
    changed = record.clone();
    changed.provenance["processes"][0]["exit_code"] = json!(1);
    assert!(validate_record(&changed).is_err());
    fs::write(
        root.join("annual-scene.rad"),
        "!embedded shell command must never execute\n",
    )
    .unwrap();
    reseal(&root, &record);
    assert!(read_run_directory(&root).is_err());
    fs::write(
        root.join("annual-scene.rad"),
        greenlens_radiance::scene_text(&record.validated).unwrap(),
    )
    .unwrap();
    let mut recipe: annual::AnnualRecipe = read_json(&root.join("annual-recipe.json")).unwrap();
    recipe.interval_indices = vec![1];
    write_json(&root.join("annual-recipe.json"), &recipe).unwrap();
    reseal(&root, &record);
    assert!(read_run_directory(&root).is_err());
    recipe.interval_indices = vec![0];
    write_json(&root.join("annual-recipe.json"), &recipe).unwrap();
    let mut bytes = fs::read(root.join("series/data-00000.f64")).unwrap();
    bytes[0..8].copy_from_slice(&f64::NAN.to_le_bytes());
    fs::write(root.join("series/data-00000.f64"), &bytes).unwrap();
    changed = record.clone();
    changed.series.chunks[0].sha256 = bytes_hash(&bytes);
    identities(&mut changed);
    write_json(&root.join("series/series.json"), &changed.series).unwrap();
    save_record(&root, &changed);
    assert!(read_run_directory(&root).is_err());
}

#[test]
fn annual_metrics_are_rederived_and_settings_cannot_relabel_old_metrics() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("annual");
    let mut record = fixture(&root, true);
    read_run_directory(&root).unwrap();
    let physical = physical_result_fingerprint(&record).unwrap();
    let old_metric = record.provenance["metric_identity"].clone();
    record.study.metrics.da_threshold_lux = 500.0;
    identities(&mut record);
    assert_eq!(physical_result_fingerprint(&record).unwrap(), physical);
    assert_ne!(record.provenance["metric_identity"], old_metric);
    // A self-consistent metadata reseal cannot turn all-dark raw data into DA=25%.
    record.metrics.as_mut().unwrap()[0].da_percent = 25.0;
    save_record(&root, &record);
    assert!(read_run_directory(&root).is_err());
}

#[test]
fn missing_current_weather_duplicate_ids_and_path_aliases_are_rejected() {
    let temp = tempdir().unwrap();
    let record = fixture(&temp.path().join("run"), false);
    let alternatives = vec![
        Alternative {
            id: "A".into(),
            model: model(),
        },
        Alternative {
            id: "B".into(),
            model: Model::alternative_b(),
        },
    ];
    let mut extension = ClimateProject::default();
    extension.studies.insert("A".into(), record.study.clone());
    assert!(validate_project_extension(&extension, &alternatives).is_err());
    let id = Uuid::new_v4().to_string();
    extension.weather.push(WeatherReference {
        id: id.clone(),
        path: format!("weather/{id}"),
        binding: record.weather_asset.binding.clone(),
    });
    validate_project_extension(&extension, &alternatives).unwrap();
    extension.weather.push(extension.weather[0].clone());
    assert!(validate_project_extension(&extension, &alternatives).is_err());
    extension.weather.pop();
    extension.weather[0].path = "../outside".into();
    assert!(validate_project_extension(&extension, &alternatives).is_err());
    let mut bad = summary(&record).unwrap();
    bad.path = "runs/../outside".into();
    assert!(read_run(temp.path(), &bad).is_err());
    let mut historical = record.clone();
    historical.schema_version = "greenlens.climate-run.future".into();
    assert!(validate_record(&historical).is_err());
}

#[test]
fn edited_nested_weather_is_reparsed_even_after_both_seals_are_updated() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("run");
    let record = fixture(&root, false);
    let weather_root = root.join("weather");
    let mut metadata: greenlens_core::weather::WeatherData =
        read_json(&weather_root.join("weather.json")).unwrap();
    metadata.hours[0].direct_normal_wh_m2 = 500.0;
    write_json(&weather_root.join("weather.json"), &metadata).unwrap();
    seal(
        &weather_root,
        &[
            "asset.json",
            "weather.json",
            "original.epw",
            "normalized.epw",
        ],
    )
    .unwrap();
    reseal(&root, &record);
    assert!(read_run_directory(&root).is_err());
}

#[test]
fn hourly_query_checks_metadata_and_requested_chunk_without_rescanning_the_year() {
    let temp = tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    let record = fixture(&scratch, true);
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    promote_run(&scratch, &project, &record.id).unwrap();
    let summary = summary(&record).unwrap();
    let run = project.join(&summary.path);
    let last = record.series.chunks.last().unwrap();
    fs::write(run.join("series").join(&last.file), [0_u8]).unwrap();
    // Display queries are deliberately bounded. Full acceptance still rejects
    // the unrelated corrupted final interval before reopening/importing a run.
    let (_, reader) = open_series_for_run(&project, &summary).unwrap();
    assert!(reader.hour(0).is_ok());
    assert!(reader.hour(HOURS_PER_YEAR - 1).is_err());
    assert!(read_run(&project, &summary).is_err());
    fs::write(run.join("results.json"), b"{}").unwrap();
    assert!(read_result_metadata(&project, &summary).is_err());
}

#[test]
fn project_summary_binds_exact_result_metadata_even_after_coherent_resealing() {
    let temp = tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    let record = fixture(&scratch, true);
    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    promote_run(&scratch, &project, &record.id).unwrap();
    let original_summary = summary(&record).unwrap();
    let run = project.join(&original_summary.path);
    read_run(&project, &original_summary).unwrap();

    // Metadata-only checks cannot independently derive annual metrics, so the
    // project must bind the accepted immutable record before any bounded query.
    let mut changed = record.clone();
    changed.metrics.as_mut().unwrap()[0].da_percent = 25.;
    validate_record(&changed).unwrap();
    save_record(&run, &changed);
    verify_seal(&run).unwrap();
    assert!(read_result_metadata(&project, &original_summary).is_err());
    assert!(open_series_for_run(&project, &original_summary).is_err());

    // Even a scientifically valid metadata replacement changes the historical
    // record identity. Its physical and comparison conditions remain distinct.
    changed = record.clone();
    changed.provenance["test_fixture"] = json!("Altered synthetic provenance after completion");
    save_record(&run, &changed);
    read_run_directory(&run).unwrap();
    let substituted = summary(&changed).unwrap();
    assert_eq!(
        substituted.physical_input_hash,
        original_summary.physical_input_hash
    );
    assert_eq!(
        substituted.comparison_fingerprint,
        original_summary.comparison_fingerprint
    );
    assert_ne!(substituted.record_sha256, original_summary.record_sha256);
    assert!(read_result_metadata(&project, &original_summary).is_err());
    assert!(read_run(&project, &original_summary).is_err());
    let mut extension = ClimateProject::default();
    append_run(&mut extension, &record).unwrap();
    assert!(append_run(&mut extension, &changed).is_err());

    let mut missing = serde_json::to_value(&original_summary).unwrap();
    missing.as_object_mut().unwrap().remove("record_sha256");
    assert!(serde_json::from_value::<ClimateRunSummary>(missing).is_err());
}

#[test]
fn zero_solar_input_rejects_positive_rgb_lux_and_missing_weather_without_repair() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("run");
    let original = fixture(&root, false);
    let (_, parsed) = weather::open(&root.join("weather")).unwrap();
    validate_zero_solar_output(&parsed.data, 0, [0., -0., 0., 0.]).unwrap();
    assert!(validate_zero_solar_output(&parsed.data, HOURS_PER_YEAR, [0.]).is_err());
    assert!(validate_zero_solar_output(&parsed.data, 0, [0., 0., 0., 1e-300]).is_err());
    // Nonzero weather input is not proof of a positive indoor result or of a
    // solar position. The shared rule neither invents nor clamps either one.
    validate_zero_solar_output(&parsed.data, 8, [0., 0., 0., 0.]).unwrap();
    validate_zero_solar_output(&parsed.data, 8, [1., 1., 1., 179.]).unwrap();
    for value in [1.0, 1e-300] {
        let mut record = original.clone();
        let mut raw = fs::read(root.join("series/data-00000.f64")).unwrap();
        let rgb = [value; 3];
        for (channel, number) in [
            rgb[0],
            rgb[1],
            rgb[2],
            greenlens_core::rgb_to_lux(rgb).unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            raw[channel * 8..(channel + 1) * 8].copy_from_slice(&number.to_le_bytes());
        }
        fs::write(root.join("series/data-00000.f64"), &raw).unwrap();
        record.series.chunks[0].sha256 = bytes_hash(&raw);
        write_json(&root.join("series/series.json"), &record.series).unwrap();
        identities(&mut record);
        save_record(&root, &record);
        let error = read_run_directory(&root).unwrap_err();
        assert!(error
            .to_string()
            .contains("Nonzero daylight output for zero DNI/DHI"));
        assert_eq!(fs::read(root.join("series/data-00000.f64")).unwrap(), raw);
    }

    // A missing required field at a dark interval is rejected by the retained
    // original parser before output validation; it is never interpreted as zero.
    let weather_root = root.join("weather");
    let text = String::from_utf8(fs::read(weather_root.join("original.epw")).unwrap()).unwrap();
    let mut lines = text.lines().map(String::from).collect::<Vec<_>>();
    let mut fields = lines[8].split(',').map(String::from).collect::<Vec<_>>();
    fields[14] = "9999".into();
    lines[8] = fields.join(",");
    fs::write(
        weather_root.join("original.epw"),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();
    seal(
        &weather_root,
        &[
            "asset.json",
            "weather.json",
            "original.epw",
            "normalized.epw",
        ],
    )
    .unwrap();
    reseal(&root, &original);
    let error = read_run_directory(&root).unwrap_err();
    assert!(error.to_string().contains("missing sentinels are rejected"));
}

#[test]
fn occupied_zero_solar_intervals_remain_in_annual_metric_denominator() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("annual");
    let mut record = fixture(&root, true);
    let mut weights = vec![0.; HOURS_PER_YEAR];
    weights[0] = 0.5; // retained zero-solar input and exact zero illuminance
    weights[8] = 0.5; // synthetic daylight input, manually assigned positive RGB
    record.study.metrics.occupancy = greenlens_core::study::Occupancy::Mask { weights };
    let mut raw = fs::read(root.join("series/data-00000.f64")).unwrap();
    let rgb = [600. / 179.; 3];
    let sensors = record.series.sensor_ids.len();
    for sensor in 0..sensors {
        let start = (8 * sensors + sensor) * 32;
        for (channel, number) in [
            rgb[0],
            rgb[1],
            rgb[2],
            greenlens_core::rgb_to_lux(rgb).unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            raw[start + channel * 8..start + (channel + 1) * 8]
                .copy_from_slice(&number.to_le_bytes());
        }
    }
    fs::write(root.join("series/data-00000.f64"), &raw).unwrap();
    record.series.chunks[0].sha256 = bytes_hash(&raw);
    write_json(&root.join("series/series.json"), &record.series).unwrap();
    record.metrics = Some(
        SeriesReader::open(&root.join("series"))
            .unwrap()
            .metrics(&record.study.metrics)
            .unwrap(),
    );
    identities(&mut record);
    save_record(&root, &record);
    let accepted = read_run_directory(&root).unwrap();
    for metric in accepted.metrics.unwrap() {
        assert_eq!(
            [
                metric.da_percent,
                metric.cda_percent,
                metric.udi_percent,
                metric.underlit_percent,
                metric.overlit_percent
            ],
            [50., 50., 50., 50., 0.]
        );
    }
}

#[test]
fn direct_batch_partition_covers_selected_suns_once_with_bounded_exact_shapes() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("mock");
    fixture(&root, false);
    let mut recipe: annual::AnnualRecipe = read_json(&root.join("annual-recipe.json")).unwrap();
    recipe.profile = annual::AnnualProfile::reference();
    recipe.sensor_count = annual::MAX_SENSORS;
    recipe.interval_indices = vec![3, 5, 8];
    recipe.solar_interval_indices = vec![2, 3, 5, 8, 9];
    // Reference at the sensor cap has exactly one solar interval per batch.
    recipe.direct_batches = [3, 5, 8]
        .map(|interval| annual::DirectBatch {
            interval_indices: vec![interval],
            attempted_rays: annual::MAX_DIRECT_BATCH_RAYS,
            emitted_rays: 1,
            culled_rays: annual::MAX_DIRECT_BATCH_RAYS - 1,
            input_sha256: bytes_hash(b"MOCK ray input; not real engine evidence"),
            output_sha256: bytes_hash(b"MOCK ray output; not real engine evidence"),
        })
        .to_vec();
    validate_direct_batches(&recipe).unwrap();
    let original = recipe.clone();
    recipe.direct_batches.swap(0, 1);
    assert!(validate_direct_batches(&recipe).is_err());
    recipe = original.clone();
    recipe.direct_batches.pop();
    assert!(validate_direct_batches(&recipe).is_err());
    recipe = original.clone();
    recipe.direct_batches[0].interval_indices = vec![2]; // context-only sun
    assert!(validate_direct_batches(&recipe).is_err());
    recipe = original.clone();
    recipe.direct_batches[0].culled_rays = usize::MAX;
    assert!(validate_direct_batches(&recipe).is_err());
    recipe = original.clone();
    recipe.direct_batches[0].attempted_rays += 1;
    assert!(validate_direct_batches(&recipe).is_err());
    recipe = original.clone();
    recipe.direct_batches[0].input_sha256 = "A".repeat(64);
    assert!(validate_direct_batches(&recipe).is_err());
    recipe = original;
    recipe.direct_batches[0].emitted_rays = 0;
    recipe.direct_batches[0].culled_rays = annual::MAX_DIRECT_BATCH_RAYS;
    assert!(validate_direct_batches(&recipe).is_err()); // nonempty input/output hashes
    recipe.direct_batches[0].input_sha256 = bytes_hash(b"");
    recipe.direct_batches[0].output_sha256 = bytes_hash(b"");
    validate_direct_batches(&recipe).unwrap();
}

/// Rebind intentionally mocked files so rejection tests exercise the inner
/// contract rather than stopping at a stale checksum. No engine is executed.
fn save_mock_recipe(root: &Path, record: &mut ClimateRunRecord, recipe: &annual::AnnualRecipe) {
    write_json(&root.join("annual-recipe.json"), recipe).unwrap();
    write_json(&root.join("annual-logs.json"), &recipe.processes).unwrap();
    record.provenance["processes"] = serde_json::to_value(&recipe.processes).unwrap();
    for (key, name) in [
        ("recipe_sha256", "annual-recipe.json"),
        ("solar_sources_sha256", "annual-suns.rad"),
        ("solar_modifiers_sha256", "annual-sun-modifiers.txt"),
    ] {
        record.provenance[key] = json!(bytes_hash(&fs::read(root.join(name)).unwrap()));
    }
    let mut resources: Value = read_json(&root.join("annual-resources.json")).unwrap();
    let original = resources["processes"][0].clone();
    resources["processes"] = json!(recipe
        .processes
        .iter()
        .map(|process| {
            let mut item = original.clone();
            item["executable"] = json!(process.executable);
            item["started_utc"] = json!(process.started_utc);
            item["finished_utc"] = json!(process.finished_utc);
            item
        })
        .collect::<Vec<_>>());
    write_json(&root.join("annual-resources.json"), &resources).unwrap();
    identities(record);
    save_record(root, record);
}

fn mock_direct_processes(record: &ClimateRunRecord, recipe: &mut annual::AnnualRecipe) {
    recipe.processes = expected_processes(record, recipe)
        .unwrap()
        .into_iter()
        .map(|(executable, arguments)| ProcessRecord {
            executable,
            arguments,
            exit_code: 0,
            started_utc: "2026-01-01T00:00:00Z".into(),
            finished_utc: "2026-01-01T00:00:00Z".into(),
            elapsed_ms: 0,
            stderr: "MOCK: no direct ray or engine process was executed".into(),
        })
        .collect();
}

#[test]
fn direct_recipe_sources_and_trace_processes_are_validated_after_coherent_reseal() {
    for method in [
        annual::LEGACY_METHOD_VERSION,
        annual::EXPLICIT_METHOD_VERSION,
        annual::METHOD_VERSION,
    ] {
        let temp = tempdir().unwrap();
        let root = temp.path().join("mock");
        let mut record = fixture_at_method(&root, false, 8, method);
        let mut recipe: annual::AnnualRecipe = read_json(&root.join("annual-recipe.json")).unwrap();
        recipe.solar_interval_indices = vec![8];
        let attempted = recipe.sensor_count * recipe.profile.direct_solar_samples;
        recipe.direct_batches = vec![annual::DirectBatch {
            interval_indices: vec![8],
            attempted_rays: attempted,
            emitted_rays: 1,
            culled_rays: attempted - 1,
            input_sha256: bytes_hash(b"MOCK ray input"),
            output_sha256: bytes_hash(b"MOCK ray output"),
        }];
        let source =
        "void light solar510\n0\n0\n3 1 1 1\nsolar510 source sun510\n0\n0\n4 0 -0.8 0.6 0.533\n";
        fs::write(root.join("annual-suns.rad"), source).unwrap();
        fs::write(root.join("annual-sun-modifiers.txt"), "solar510\n").unwrap();
        mock_direct_processes(&record, &mut recipe);
        assert_eq!(
            recipe.processes.len(),
            if method == annual::METHOD_VERSION {
                10
            } else {
                9
            }
        );
        assert_eq!(recipe.processes.last().unwrap().executable, "rtrace.exe");
        save_mock_recipe(&root, &mut record, &recipe);
        read_run_directory(&root).unwrap();

        // A coherently resealed recipe must not describe the other method's
        // reflected-solar calculation, even though finite rays are unchanged.
        let mut wrong_policy = recipe.clone();
        wrong_policy.direct_policy = if method == annual::LEGACY_METHOD_VERSION {
            annual::DIRECT_POLICY
        } else {
            annual::LEGACY_DIRECT_POLICY
        }
        .into();
        save_mock_recipe(&root, &mut record, &wrong_policy);
        assert!(read_run_directory(&root)
            .unwrap_err()
            .to_string()
            .contains("direct policy"));
        save_mock_recipe(&root, &mut record, &recipe);

        // A retained process list cannot quietly increase bounce depth of the
        // finite-direction transmission trace, even with all outer hashes updated.
        let mut changed = recipe.clone();
        let arguments = &mut changed.processes.last_mut().unwrap().arguments;
        let position = arguments.iter().position(|arg| arg == "-ab").unwrap();
        arguments[position + 1] = "1".into();
        save_mock_recipe(&root, &mut record, &changed);
        assert!(read_run_directory(&root)
            .unwrap_err()
            .to_string()
            .contains("Recorded commands"));
        changed = recipe.clone();
        changed.direct_batches[0].interval_indices[0] = 7;
        save_mock_recipe(&root, &mut record, &changed);
        assert!(read_run_directory(&root)
            .unwrap_err()
            .to_string()
            .contains("Direct batch"));
        save_mock_recipe(&root, &mut record, &recipe);
        fs::write(
            root.join("annual-suns.rad"),
            format!("{source}!unsupported command\n"),
        )
        .unwrap();
        save_mock_recipe(&root, &mut record, &recipe);
        assert!(read_run_directory(&root)
            .unwrap_err()
            .to_string()
            .contains("solar definition count"));

        // Empty-cull batches remain explicit provenance, but must have no rtrace.
        fs::write(root.join("annual-suns.rad"), source).unwrap();
        recipe.direct_batches[0].emitted_rays = 0;
        recipe.direct_batches[0].culled_rays = attempted;
        recipe.direct_batches[0].input_sha256 = bytes_hash(b"");
        recipe.direct_batches[0].output_sha256 = bytes_hash(b"");
        mock_direct_processes(&record, &mut recipe);
        assert_eq!(
            recipe.processes.len(),
            if method == annual::METHOD_VERSION {
                9
            } else {
                8
            }
        );
        save_mock_recipe(&root, &mut record, &recipe);
        read_run_directory(&root).unwrap();

        // A sun generated only for preceding dew-point context is never assigned a
        // direct batch for the selected output hour. The base matrix passes remain.
        recipe.solar_interval_indices = vec![6];
        recipe.direct_batches.clear();
        fs::write(root.join("annual-suns.rad"), source.replace("510", "390")).unwrap();
        fs::write(root.join("annual-sun-modifiers.txt"), "solar390\n").unwrap();
        mock_direct_processes(&record, &mut recipe);
        save_mock_recipe(&root, &mut record, &recipe);
        read_run_directory(&root).unwrap();
    }
}

#[test]
fn immutable_recipe_and_solar_file_pins_reject_resealing_without_record_changes() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("mock");
    let record = fixture(&root, false);
    for key in [
        "recipe_sha256",
        "solar_sources_sha256",
        "solar_modifiers_sha256",
    ] {
        let mut changed = record.clone();
        changed.provenance[key] = Value::Null;
        assert!(validate_record(&changed).is_err());
    }
    for file in [
        "annual-recipe.json",
        "annual-suns.rad",
        "annual-sun-modifiers.txt",
    ] {
        let original = fs::read(root.join(file)).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n");
        fs::write(root.join(file), changed).unwrap();
        reseal(&root, &record);
        assert!(
            read_run_directory(&root).is_err(),
            "mutable {file} accepted"
        );
        fs::write(root.join(file), original).unwrap();
        reseal(&root, &record);
    }
    read_run_directory(&root).unwrap();
}

#[test]
fn historical_methods_two_three_keep_exact_profile_evidence_and_metric_derivation() {
    for method in [
        annual::LEGACY_METHOD_VERSION,
        annual::EXPLICIT_METHOD_VERSION,
    ] {
        let temporary = tempdir().unwrap();
        let scratch = temporary.path().join("historical-mock");
        let project = temporary.path().join("portable");
        let record = fixture_for_method(&scratch, true, method);
        // Frozen serialization, independent of whichever profile is current later.
        let expected_profile = json!({
            "name":"shipping", "sky_basis_mf":2, "ambient_bounces":8,
            "ambient_divisions":131072, "direct_ambient_divisions":131072,
            "direct_solar_samples":1024, "ambient_supersamples":0,
            "ambient_accuracy":0.0, "ambient_resolution":256, "ambient_value":[0.0,0.0,0.0],
            "limit_weight":1e-9, "ray_depth":16, "direct_threshold":0.0,
            "direct_certainty":1.0, "direct_jitter":0.0, "direct_size":0.05,
            "specular_threshold":0.0, "specular_sampling":0.0,
            "uncorrelated_random_sampling":false, "workers":1
        });
        let expected_profile = if method == annual::LEGACY_METHOD_VERSION {
            expected_profile
        } else {
            serde_json::from_str::<Value>(include_str!(
                "../../../tests/fixtures/climate-method3-contract.json"
            ))
            .unwrap()["profile"]
                .clone()
        };
        assert_eq!(record.provenance["profile"], expected_profile);
        let serialized = serde_json::to_vec(&record).unwrap();
        let reference = summary(&record).unwrap();
        let original_files = evidence_files(&record)
            .into_iter()
            .chain(["checksums.json".into()])
            .map(|name| {
                let data = fs::read(scratch.join(&name)).unwrap();
                (name, data)
            })
            .collect::<Vec<_>>();
        read_run_directory(&scratch).unwrap();
        fs::create_dir(&project).unwrap();
        promote_run(&scratch, &project, &record.id).unwrap();
        fs::rename(&scratch, temporary.path().join("original-unavailable")).unwrap();
        let restored = read_run(&project, &reference).unwrap();
        assert_eq!(serde_json::to_vec(&restored).unwrap(), serialized);
        assert_eq!(summary(&restored).unwrap(), reference);
        let derived =
            crate::studies::derive_metrics(&project, &reference, record.study.metrics.clone())
                .unwrap();
        assert_eq!(
            serde_json::to_value(derived.metrics).unwrap(),
            json!(record.metrics.unwrap())
        );
        assert_eq!(
            json!(derived.metric_identity),
            record.provenance["metric_identity"]
        );
        let mut changed = record.study.metrics.clone();
        changed.da_threshold_lux = 500.0;
        let derived_changed =
            crate::studies::derive_metrics(&project, &reference, changed).unwrap();
        assert_ne!(
            json!(derived_changed.metric_identity),
            record.provenance["metric_identity"]
        );
        for (name, original) in original_files {
            assert_eq!(
                fs::read(project.join(&reference.path).join(name)).unwrap(),
                original
            );
        }
    }
}

#[test]
fn historical_method_rejects_null_profile_alias_and_cross_method_relabeling() {
    for method in [
        annual::LEGACY_METHOD_VERSION,
        annual::EXPLICIT_METHOD_VERSION,
    ] {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("historical-mock");
        let original = fixture_for_method(&root, false, method);
        let mut null_alias = original.clone();
        null_alias.provenance["profile"]["coefficient_receiver_scale"] = Value::Null;
        identities(&mut null_alias);
        assert!(validate_record(&null_alias)
            .unwrap_err()
            .to_string()
            .contains("canonical"));
        let mut unknown = original.clone();
        unknown.provenance["profile"]["ambient_divisions"] = json!(131073);
        identities(&mut unknown);
        assert!(validate_record(&unknown).is_err());
        let mut other_profile = original.clone();
        other_profile.provenance["profile"] = json!(annual::AnnualProfile::shipping());
        identities(&mut other_profile);
        assert!(validate_record(&other_profile).is_err());
        let mut other_engine = original.clone();
        other_engine.provenance["engine"] = json!(engine(annual::METHOD_VERSION));
        identities(&mut other_engine);
        assert!(validate_record(&other_engine).is_err());

        let recipe: annual::AnnualRecipe = read_json(&root.join("annual-recipe.json")).unwrap();
        let mut record = original;
        for mutation in 0..4 {
            let mut changed = recipe.clone();
            match mutation {
                0 => changed.schema_version = annual::RECIPE_SCHEMA_VERSION.into(),
                1 => changed.method_version = annual::METHOD_VERSION.into(),
                2 => changed.ground_policy = annual::GROUND_POLICY.into(),
                _ => changed.method_version = "unsupported-method/999".into(),
            }
            save_mock_recipe(&root, &mut record, &changed);
            assert!(
                read_run_directory(&root).is_err(),
                "Mutation {mutation} accepted"
            );
        }
        save_mock_recipe(&root, &mut record, &recipe);
        read_run_directory(&root).unwrap();
    }
}

#[test]
fn historical_methods_reject_new_coefficient_fields_negative_depth_and_members() {
    for method in [
        annual::LEGACY_METHOD_VERSION,
        annual::EXPLICIT_METHOD_VERSION,
    ] {
        let temporary = tempdir().unwrap();
        let original = fixture_for_method(temporary.path(), false, method);
        for field in [
            "coefficient_primary_repetitions",
            "coefficient_receiver_scale",
        ] {
            let mut changed = original.clone();
            changed.provenance["profile"][field] = Value::Null;
            identities(&mut changed);
            assert!(validate_record(&changed)
                .unwrap_err()
                .to_string()
                .contains("canonical"));
            changed.provenance["profile"][field] = json!(100);
            identities(&mut changed);
            assert!(validate_record(&changed).is_err());
        }
        let mut changed = original.clone();
        changed.provenance["profile"]["ray_depth"] = json!(-16);
        identities(&mut changed);
        assert!(validate_record(&changed).is_err());
        for name in [
            "annual-coefficient-receiver.rad",
            "annual-coefficient-sensors.pts",
        ] {
            fs::write(
                temporary.path().join(name),
                b"unexpected later-method input",
            )
            .unwrap();
            let mut files = evidence_files(&original);
            files.push(name.into());
            seal(
                temporary.path(),
                &files.iter().map(String::as_str).collect::<Vec<_>>(),
            )
            .unwrap();
            assert!(read_run_directory(temporary.path())
                .unwrap_err()
                .to_string()
                .contains("required members"));
            fs::remove_file(temporary.path().join(name)).unwrap();
            reseal(temporary.path(), &original);
        }
        read_run_directory(temporary.path()).unwrap();
    }
}

#[test]
fn repeated_coefficient_inputs_are_regenerated_after_coherent_reseal() {
    let temporary = tempdir().unwrap();
    let root = temporary.path();
    let record = fixture(root, false);
    assert_eq!(record.provenance["profile"]["ray_depth"], json!(-16));
    let ordinary = fs::read(root.join("annual-sensors.pts")).unwrap();
    let unit_receiver = fs::read(root.join("annual-receiver.rad")).unwrap();
    assert_eq!(unit_receiver, annual::RECEIVER_TEXT.as_bytes());
    for file in [
        "annual-coefficient-receiver.rad",
        "annual-coefficient-sensors.pts",
    ] {
        let original = fs::read(root.join(file)).unwrap();
        fs::remove_file(root.join(file)).unwrap();
        assert!(read_run_directory(root).is_err());
        fs::write(root.join(file), &original).unwrap();
        let changed = if file.ends_with("sensors.pts") {
            let text = String::from_utf8(original.clone()).unwrap();
            let mut lines = text.lines().collect::<Vec<_>>();
            let repeats = record.provenance["profile"]["coefficient_primary_repetitions"]
                .as_u64()
                .unwrap() as usize;
            assert!(lines.len() > repeats && lines[0] != lines[repeats]);
            lines.swap(0, repeats);
            format!("{}\n", lines.join("\n")).into_bytes()
        } else {
            String::from_utf8(original.clone())
                .unwrap()
                .replace("100", "200")
                .into_bytes()
        };
        assert_ne!(changed, original);
        fs::write(root.join(file), changed).unwrap();
        reseal(root, &record);
        assert!(read_run_directory(root)
            .unwrap_err()
            .to_string()
            .contains("Scaled receiver/repeated sensors"));
        fs::write(root.join(file), original).unwrap();
        reseal(root, &record);
    }
    assert_eq!(fs::read(root.join("annual-sensors.pts")).unwrap(), ordinary);
    assert_eq!(
        fs::read(root.join("annual-receiver.rad")).unwrap(),
        unit_receiver
    );
    read_run_directory(root).unwrap();
}

#[test]
fn historical_process_builders_match_independent_frozen_contracts() {
    // Method2 uses actual candidate-01 native export arrays. Method3 uses
    // frozen438ff22 source arrays, explicitly not native execution evidence.
    for (text, method) in [
        (
            include_str!("../../../tests/fixtures/climate-method2-contract.json"),
            annual::LEGACY_METHOD_VERSION,
        ),
        (
            include_str!("../../../tests/fixtures/climate-method3-contract.json"),
            annual::EXPLICIT_METHOD_VERSION,
        ),
    ] {
        let snapshot: Value = serde_json::from_str(text).unwrap();
        let temporary = tempdir().unwrap();
        let record = fixture_for_method(temporary.path(), false, method);
        let mut recipe: annual::AnnualRecipe =
            read_json(&temporary.path().join("annual-recipe.json")).unwrap();
        assert_eq!(record.provenance["profile"], snapshot["profile"]);
        if method == annual::EXPLICIT_METHOD_VERSION {
            assert_eq!(
                json!(annual::AnnualProfile::reference_for_method(method).unwrap()),
                snapshot["reference_profile"]
            );
        }
        assert_eq!(
            record.provenance["adapter_version"],
            snapshot["adapter_version"]
        );
        assert_eq!(json!(recipe.method_version), snapshot["method_version"]);
        assert_eq!(
            json!(recipe.schema_version),
            snapshot["recipe_schema_version"]
        );
        assert_eq!(json!(recipe.ground_policy), snapshot["ground_policy"]);
        assert_eq!(json!(recipe.direct_policy), snapshot["direct_policy"]);
        // Match the exported 24-sensor process shape without fabricating a result
        // vector. Only the recognized command builder is exercised here.
        recipe.sensor_count = snapshot["sensor_count"].as_u64().unwrap() as usize;
        recipe.solar_interval_indices = vec![8];
        let attempted = recipe.sensor_count * recipe.profile.direct_solar_samples;
        recipe.direct_batches = vec![annual::DirectBatch {
            interval_indices: vec![8],
            attempted_rays: attempted,
            emitted_rays: 1,
            culled_rays: attempted - 1,
            input_sha256: bytes_hash(b"MOCK"),
            output_sha256: bytes_hash(b"MOCK"),
        }];
        let expected = expected_processes(&record, &recipe)
            .unwrap()
            .into_iter()
            .map(|(executable, arguments)| json!({"executable":executable,"arguments":arguments}))
            .collect::<Vec<_>>();
        assert_eq!(json!(expected), snapshot["processes"]);
    }
}

#[test]
fn method4_contract_preserves_profiles_inputs_members_and_executed_command_prefix() {
    // Captured before external-primary integration. Native jobs supplying the
    // first nine command arrays failed assembly; this fixture never claims a
    // completed simulation. The final finite-direct command is source-only.
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/climate-method4-contract.json"
    ))
    .unwrap();
    let method = snapshot["method_version"].as_str().unwrap();
    for (profile_key, command_key) in [
        ("profile", "processes"),
        ("reference_profile", "reference_processes"),
    ] {
        let profile: annual::AnnualProfile =
            serde_json::from_value(snapshot[profile_key].clone()).unwrap();
        profile.validate_for_method(method).unwrap();
        assert_eq!(json!(profile), snapshot[profile_key]);
        let frozen = if profile_key == "profile" {
            annual::AnnualProfile::shipping_for_method(method).unwrap()
        } else {
            annual::AnnualProfile::reference_for_method(method).unwrap()
        };
        assert_eq!(profile, frozen);
        assert_eq!(profile.ray_depth, -16);
        assert!(snapshot[profile_key]
            .get("solar_ambient_divisions")
            .is_none());
        assert_eq!(
            profile.coefficient_receiver_text(),
            snapshot["coefficient_receiver"].as_str().unwrap()
        );
        let repeated = profile
            .coefficient_sensor_text(snapshot["authoritative_sensor_text"].as_str().unwrap())
            .unwrap();
        let repetitions = profile.coefficient_primary_repetitions.unwrap();
        assert_eq!(repeated.lines().count(), 2 * repetitions);
        assert_eq!(
            json!(bytes_hash(repeated.as_bytes())),
            snapshot["coefficient_sensor_sha256"][repetitions.to_string()]
        );

        let temporary = tempdir().unwrap();
        let record = fixture_for_method(temporary.path(), false, method);
        let mut recipe: annual::AnnualRecipe =
            read_json(&temporary.path().join("annual-recipe.json")).unwrap();
        recipe.profile = profile;
        assert_eq!(
            json!(recipe.schema_version),
            snapshot["recipe_schema_version"]
        );
        assert_eq!(json!(recipe.ground_policy), snapshot["ground_policy"]);
        assert_eq!(json!(recipe.direct_policy), snapshot["direct_policy"]);
        assert_eq!(
            record.provenance["adapter_version"],
            snapshot["adapter_version"]
        );
        assert_eq!(json!(annual::RECEIVER_TEXT), snapshot["unit_receiver"]);
        let mut members = evidence_files(&record);
        members.retain(|name| {
            !record
                .series
                .chunks
                .iter()
                .any(|chunk| name == &format!("series/{}", chunk.file))
        });
        members.sort();
        assert_eq!(
            json!(members),
            snapshot["required_evidence_members_excluding_chunks"]
        );
        recipe.sensor_count = snapshot["sensor_count"].as_u64().unwrap() as usize;
        recipe.solar_interval_indices = vec![8];
        let attempted = recipe.sensor_count * recipe.profile.direct_solar_samples;
        recipe.direct_batches = vec![annual::DirectBatch {
            interval_indices: vec![8],
            attempted_rays: attempted,
            emitted_rays: 1,
            culled_rays: attempted - 1,
            input_sha256: bytes_hash(b"MOCK"),
            output_sha256: bytes_hash(b"MOCK"),
        }];
        let commands = expected_processes(&record, &recipe)
            .unwrap()
            .into_iter()
            .map(|(executable, arguments)| json!({"executable":executable,"arguments":arguments}))
            .collect::<Vec<_>>();
        assert_eq!(json!(commands), snapshot[command_key]);
        recipe.solar_interval_indices.clear();
        recipe.direct_batches.clear();
        let no_solar = expected_processes(&record, &recipe)
            .unwrap()
            .into_iter()
            .map(|(executable, arguments)| json!({"executable":executable,"arguments":arguments}))
            .collect::<Vec<_>>();
        assert_eq!(json!(no_solar), json!(&commands[..6]));
    }
}

#[test]
fn historical_weather_navigation_uses_embedded_assets_without_current_weather_references() {
    for method in [
        annual::LEGACY_METHOD_VERSION,
        annual::EXPLICIT_METHOD_VERSION,
        annual::METHOD_VERSION,
    ] {
        let scratch = tempdir().unwrap();
        let root = tempdir().unwrap();
        let record = fixture_for_method(scratch.path(), false, method);
        let reference = summary(&record).unwrap();
        promote_run(scratch.path(), root.path(), &record.id).unwrap();
        drop(scratch);
        let mut extension = ClimateProject::default();
        append_run(&mut extension, &record).unwrap();
        let mut project = crate::example();
        project.schema_version = "0.3.0".into();
        project.climate = Some(extension);
        write_json(&root.path().join("project.json"), &project).unwrap();
        let view = crate::open_project(root.path()).unwrap();
        assert!(view.project.climate.as_ref().unwrap().weather.is_empty());
        assert!(view
            .project
            .climate
            .as_ref()
            .unwrap()
            .studies
            .values()
            .all(|study| matches!(study.calculation, StudyMode::StaticOvercast)));
        assert!(!root.path().join("weather").exists());
        let directory = root.path().join(&reference.path);
        let before = evidence_files(&record)
            .into_iter()
            .chain(["checksums.json".into()])
            .map(|name| {
                let data = fs::read(directory.join(&name)).unwrap();
                (name, data)
            })
            .collect::<Vec<_>>();
        for (month, day, hour, index) in [(1, 1, 1, 0), (12, 31, 24, 8759)] {
            let found =
                run_weather_find_interval(root.path(), &reference, month, day, hour).unwrap();
            let direct = run_weather_interval(root.path(), &reference, index).unwrap();
            assert_eq!(json!(found), json!(direct));
            assert_eq!(found.run_id, record.id);
            assert_eq!(found.record_sha256, reference.record_sha256);
            assert_eq!(found.binding, record.weather_asset.binding);
            assert_eq!(
                (found.hour.month, found.hour.day, found.hour.hour_ending),
                (month, day, hour)
            );
            assert_eq!(found.hour.index, index);
            assert_eq!(found.interval.local_start_minute, (index * 60) as i32);
        }
        for (name, original) in before {
            assert_eq!(fs::read(directory.join(name)).unwrap(), original);
        }
    }
}

#[test]
fn historical_weather_queries_reject_bad_labels_and_record_pins_without_scanning_result_chunks() {
    let scratch = tempdir().unwrap();
    let root = tempdir().unwrap();
    let record = fixture(scratch.path(), false);
    let reference = summary(&record).unwrap();
    promote_run(scratch.path(), root.path(), &record.id).unwrap();
    // Weather queries use compact metadata and retained EPW, not raw results.
    let chunk = root
        .path()
        .join(&reference.path)
        .join("series")
        .join(&record.series.chunks[0].file);
    let bytes = fs::read(&chunk).unwrap();
    fs::remove_file(&chunk).unwrap();
    run_weather_interval(root.path(), &reference, 8759).unwrap();
    run_weather_find_interval(root.path(), &reference, 1, 1, 1).unwrap();
    assert!(open_series_for_run(root.path(), &reference)
        .unwrap()
        .1
        .hour(0)
        .is_err());
    fs::write(chunk, bytes).unwrap();
    for (month, day, hour) in [
        (0, 1, 1),
        (13, 1, 1),
        (2, 29, 1),
        (4, 31, 1),
        (1, 1, 0),
        (1, 1, 25),
    ] {
        assert!(run_weather_find_interval(root.path(), &reference, month, day, hour).is_err());
    }
    assert!(run_weather_interval(root.path(), &reference, 8760).is_err());
    let mut changed = reference.clone();
    changed.record_sha256 = "f".repeat(64);
    assert!(run_weather_interval(root.path(), &changed, 0).is_err());
    changed.path = "runs/../outside".into();
    assert!(run_weather_find_interval(root.path(), &changed, 1, 1, 1).is_err());
}

#[test]
fn historical_weather_reparses_and_rejects_coherently_resealed_asset_substitution() {
    let scratch = tempdir().unwrap();
    let root = tempdir().unwrap();
    let record = fixture_for_method(scratch.path(), false, annual::LEGACY_METHOD_VERSION);
    let reference = summary(&record).unwrap();
    promote_run(scratch.path(), root.path(), &record.id).unwrap();
    let directory = root.path().join(&reference.path);
    let weather = directory.join("weather");
    let mut changed = record.weather_asset.clone();
    changed.source.citation = "Different source attribution after resealing".into();
    write_json(&weather.join("asset.json"), &changed).unwrap();
    let seal_weather = || {
        seal(
            &weather,
            &[
                "asset.json",
                "weather.json",
                "original.epw",
                "normalized.epw",
            ],
        )
        .unwrap()
    };
    seal_weather();
    reseal(&directory, &record);
    assert!(run_weather_interval(root.path(), &reference, 0)
        .unwrap_err()
        .to_string()
        .contains("immutable run snapshot"));
    write_json(&weather.join("asset.json"), &record.weather_asset).unwrap();
    let mut metadata: greenlens_core::weather::WeatherData =
        read_json(&weather.join("weather.json")).unwrap();
    metadata.hours[0].direct_normal_wh_m2 = 500.0;
    write_json(&weather.join("weather.json"), &metadata).unwrap();
    seal_weather();
    reseal(&directory, &record);
    assert!(run_weather_find_interval(root.path(), &reference, 1, 1, 1).is_err());
}
