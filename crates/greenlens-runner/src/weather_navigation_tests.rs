//! Original synthetic weather tests navigation and persistence only. No engine
//! execution or climatic/astronomical accuracy is asserted here.
use super::*;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn fixture(longitude: &str, offset: &str) -> (tempfile::TempDir, PathBuf, String, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("portable project");
    let input = temporary.path().join("input.epw");
    let asset = temporary.path().join("imported asset");
    let original = String::from_utf8(greenlens_core::weather::synthetic_epw()).unwrap();
    let (header, remaining) = original.split_once('\n').unwrap();
    let mut columns = header.split(',').collect::<Vec<_>>();
    columns[7] = longitude;
    columns[8] = offset;
    fs::write(&input, format!("{}\n{remaining}", columns.join(","))).unwrap();
    weather::import(
        &input,
        &asset,
        WeatherSourceProfile::EpwHourEnding,
        weather::WeatherSource {
            citation: "Original non-climatic navigation fixture".into(),
            source_url: None,
            license: "Apache-2.0".into(),
            rights_note: "Original synthetic test data; no climatic claim".into(),
            synthetic_non_climatic: true,
        },
    )
    .unwrap();
    let saved = crate::save_project(&project, &crate::example(), None).unwrap();
    let attached =
        import_weather_asset(&project, &saved.project, Some(&saved.revision), &asset).unwrap();
    let id = attached.project.climate.unwrap().weather[0].id.clone();
    (temporary, project, id, input)
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), files);
            } else {
                files.insert(
                    entry.path().strip_prefix(root).unwrap().into(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn first_and_final_hour_use_the_retained_axis_without_touching_inputs() {
    let (_temporary, project, id, input) = fixture("114.17", "8");
    let original = fs::read(&input).unwrap();
    let before = snapshot(&project);
    let first = weather_find_interval(&project, &id, 1, 1, 1).unwrap();
    assert_eq!(first.hour.index, 0);
    assert_eq!(first.interval.local_start_minute, 0);
    assert_eq!(first.interval.local_end_minute, 60);
    assert_eq!(first.interval.local_representative_minute, 30);
    assert_eq!(first.interval.utc_start_minute, -480);
    assert_eq!(first.interval.utc_representative_minute, -450);
    let last = weather_find_interval(&project, &id, 12, 31, 24).unwrap();
    assert_eq!(last.hour.index, 8759);
    assert_eq!(
        (last.hour.month, last.hour.day, last.hour.hour_ending),
        (12, 31, 24)
    );
    assert_eq!(last.interval.local_end_minute, 525600);
    assert_eq!(last.interval.utc_end_minute, 525120);
    assert_eq!(
        serde_json::to_value(&last).unwrap(),
        serde_json::to_value(weather_interval(&project, &id, 8759).unwrap()).unwrap()
    );
    assert_eq!(snapshot(&project), before);
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn invalid_dates_hours_and_unknown_assets_are_rejected_without_normalization() {
    let (_temporary, project, id, _) = fixture("0", "0");
    let before = snapshot(&project);
    for (month, day, hour) in [
        (0, 1, 1),
        (13, 1, 1),
        (1, 0, 1),
        (1, 32, 1),
        (1, 1, 0),
        (1, 1, 25),
        (2, 29, 1),
        (2, 30, 24),
        (4, 31, 12),
    ] {
        assert!(
            weather_find_interval(&project, &id, month, day, hour).is_err(),
            "invalid selection {month}/{day}, hour {hour} was accepted"
        );
    }
    assert!(weather_find_interval(&project, "not-retained", 1, 1, 1)
        .unwrap_err()
        .to_string()
        .contains("not retained"));
    assert!(weather_interval(&project, &id, 8760).is_err());
    assert_eq!(snapshot(&project), before);
}

#[test]
fn longitude_and_positive_or_negative_fixed_offsets_do_not_shift_selected_labels() {
    for (longitude, offset, offset_minutes) in [("114.17", "5.5", 330), ("-73.98", "-5", -300)] {
        let (_temporary, project, id, _) = fixture(longitude, offset);
        let first = weather_find_interval(&project, &id, 1, 1, 1).unwrap();
        assert_eq!(
            (
                first.hour.index,
                first.hour.month,
                first.hour.day,
                first.hour.hour_ending
            ),
            (0, 1, 1, 1)
        );
        assert_eq!(first.interval.local_representative_minute, 30);
        assert_eq!(
            first.interval.utc_representative_minute,
            30 - offset_minutes
        );
        let end = weather_find_interval(&project, &id, 12, 31, 24).unwrap();
        assert_eq!(end.hour.index, 8759);
        assert_eq!(end.interval.local_end_minute, 525600);
        assert_eq!(end.interval.utc_end_minute, 525600 - offset_minutes);
    }
}

#[test]
fn lookup_revalidates_retained_bytes_and_rejects_tampering() {
    let (_temporary, project, id, _) = fixture("0", "0");
    let retained = project.join("weather").join(&id);
    fs::write(retained.join("original.epw"), b"modified external weather").unwrap();
    let error = weather_find_interval(&project, &id, 1, 1, 1).unwrap_err();
    assert!(format!("{error:#}").contains("integrity"));
}
