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
fn engine() -> annual::AnnualEngineIdentity {
    let mut engine = annual::AnnualEngineIdentity {
        version: greenlens_radiance::ENGINE_VERSION.into(),
        adapter_version: annual::ADAPTER_VERSION.into(),
        method_version: annual::METHOD_VERSION.into(),
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
fn fixture_at(root: &Path, annual_result: bool, selected_interval: usize) -> ClimateRunRecord {
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
    let identity = engine();
    let profile = annual::AnnualProfile::shipping();
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
        provenance: json!({"schema_version":RUN_SCHEMA,"application_version":"synthetic-test","adapter_version":annual::ADAPTER_VERSION,
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
        schema_version: "greenlens.annual-recipe.v2".into(),
        method_version: annual::METHOD_VERSION.into(),
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
        ground_policy: annual::GROUND_POLICY.into(),
        direct_policy: annual::DIRECT_POLICY.into(),
        direct_batches: vec![],
        child_environment: BTreeMap::from([("RAYPATH".into(), ".;<engine>/lib".into())]),
        processes: vec![],
    };
    recipe.processes = expected_processes(&record, &recipe)
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
    fs::write(root.join("annual-sensors.pts"), sensors).unwrap();
    fs::write(root.join("annual-weather.epw"), generated).unwrap();
    fs::write(root.join("annual-receiver.rad"), annual::RECEIVER_TEXT).unwrap();
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
    let temp = tempdir().unwrap();
    let root = temp.path().join("mock");
    let mut record = fixture_at(&root, false, 8);
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
    assert_eq!(recipe.processes.len(), 9);
    assert_eq!(recipe.processes.last().unwrap().executable, "rtrace.exe");
    save_mock_recipe(&root, &mut record, &recipe);
    read_run_directory(&root).unwrap();

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
    assert_eq!(recipe.processes.len(), 8);
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
