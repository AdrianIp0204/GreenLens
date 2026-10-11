//! Original, deliberately non-climatic parser fixtures. Values are invented for
//! contract tests and must never be used as a real annual simulation example.
use greenlens_core::weather::{parse_epw, WeatherSourceProfile as Profile, MAX_EPW_BYTES};

#[test]
fn public_synthetic_example_is_explicit_nonclimatic_and_validated() {
    let source = greenlens_core::weather::synthetic_epw();
    let parsed = parse_epw(&source, Profile::EpwHourEnding).unwrap();
    assert_eq!(parsed.data.summary.intervals, 8760);
    assert_eq!(parsed.data.summary.zero_solar_intervals, 4380);
    assert_eq!(parsed.data.location.utc_offset_minutes, 0);
    assert!(parsed.data.location.city.contains("Non-climatic"));
    assert!(parsed.data.original_headers[5].contains("not measurements"));
    assert!(parsed.data.warnings.is_empty());
    assert_eq!(source, greenlens_core::weather::synthetic_epw());
}

fn synthetic(minute: u8, comment: &str) -> String {
    let mut s = format!("LOCATION,Synthetic,Test,ZZZ,Original non-climatic fixture,000000,22.3,114.17,8,18\nDESIGN CONDITIONS,0\nTYPICAL/EXTREME PERIODS,0\nGROUND TEMPERATURES,0\nHOLIDAYS/DAYLIGHT SAVING,No,0,0,0\nCOMMENTS 1,Original generated test data; not a climatic year\nCOMMENTS 2,{comment}\nDATA PERIODS,1,1,Synthetic,Monday,1/1,12/31\n");
    for (m, days) in [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        .iter()
        .enumerate()
    {
        for day in 1..=*days {
            for hour in 1..=24 {
                let (dni, dhi, ghi) = if (9..=18).contains(&hour) {
                    (300, 100, 250)
                } else {
                    (0, 0, 0)
                };
                s.push_str(&format!("{}, {},{day},{hour},{minute},?9?9?9,20,10,50,101325,9999,9999,300,{ghi},{dni},{dhi},999999,999999,999999,9999,180,2,5,5,9999,99999,9,999999999,999,0.999,999,99,999,999,99\n", 2000 + m, m + 1));
            }
        }
    }
    s
}
fn mutate(s: &str, row: usize, column: usize, value: &str) -> String {
    let mut lines: Vec<String> = s.lines().map(str::to_string).collect();
    let mut f: Vec<String> = lines[row + 8].split(',').map(str::to_string).collect();
    f[column] = value.into();
    lines[row + 8] = f.join(",");
    lines.join("\n") + "\n"
}
fn rejects(s: &str, field: &str) {
    let err = parse_epw(s.as_bytes(), Profile::EpwHourEnding)
        .unwrap_err()
        .to_string();
    assert!(err.contains(field), "expected {field}: {err}");
}

#[test]
fn full_nonleap_axis_midpoints_hour24_and_fixed_timezone() {
    let parsed = parse_epw(synthetic(60, "").as_bytes(), Profile::EpwHourEnding).unwrap();
    let data = parsed.data;
    assert_eq!(data.hours.len(), 8760);
    assert_eq!(data.hours[0].mean_irradiance_w_m2(), [0.0; 3]);
    assert_eq!(data.hours[8].mean_irradiance_w_m2(), [300.0, 100.0, 250.0]);
    assert_eq!(data.summary.zero_solar_intervals, 5110);
    let first = data.interval(0).unwrap();
    assert_eq!(
        (
            first.local_start_minute,
            first.local_end_minute,
            first.local_representative_minute
        ),
        (0, 60, 30)
    );
    assert_eq!(
        (
            first.utc_start_minute,
            first.utc_end_minute,
            first.utc_representative_minute
        ),
        (-480, -420, -450)
    );
    let midnight = data.interval(23).unwrap();
    assert_eq!(
        (
            midnight.day,
            midnight.hour_ending,
            midnight.local_end_minute
        ),
        (1, 24, 1440)
    );
    assert_eq!(
        (
            data.interval(24).unwrap().day,
            data.interval(24).unwrap().local_start_minute
        ),
        (2, 1440)
    );
    let last = data.interval(8759).unwrap();
    assert_eq!(
        (
            last.month,
            last.day,
            last.hour_ending,
            last.local_end_minute
        ),
        (12, 31, 24, 525600)
    );
    assert_eq!(data.summary.source_years_by_month[1], vec![2001]);
    assert!(!data.time_policy.source_year_is_calendar);
    assert!(!data.time_policy.applies_daylight_saving);
    assert!(data.interval(8760).is_err());
    data.binding().validate().unwrap();
}

#[test]
fn pvgis_offset_is_relative_to_interval_end_not_timezone() {
    let original = synthetic(0, "Irradiance Time Offset (h):-0.5");
    let parsed = parse_epw(original.as_bytes(), Profile::PvgisTmy).unwrap();
    assert_eq!(parsed.data.location.utc_offset_minutes, 480);
    assert_eq!(
        parsed.data.time_policy.declared_irradiance_offset_hours,
        Some(-0.5)
    );
    assert_eq!(parsed.data.hours[0].source_minute, 0);
    assert_eq!(
        parsed.data.interval(8).unwrap().local_representative_minute,
        510
    );
    let normalized = String::from_utf8(parsed.normalized_epw.clone()).unwrap();
    assert_eq!(
        normalized.lines().nth(4),
        Some("HOLIDAYS/DAYLIGHT SAVINGS,No,0,0,0")
    );
    assert_eq!(
        normalized.lines().nth(8).unwrap().split(',').nth(4),
        Some("60")
    );
    let reparsed = parse_epw(&parsed.normalized_epw, Profile::EpwHourEnding).unwrap();
    assert_eq!(parsed.data.time_axis_sha256, reparsed.data.time_axis_sha256);
    assert_eq!(
        parsed.data.hours[8].mean_irradiance_w_m2(),
        reparsed.data.hours[8].mean_irradiance_w_m2()
    );
    assert_eq!(
        parsed.data.hours[8].dew_point_c,
        reparsed.data.hours[8].dew_point_c
    );
    assert_ne!(parsed.data.original_sha256, parsed.data.normalized_sha256);
    assert_ne!(
        parsed.data.binding().interpretation,
        reparsed.data.binding().interpretation
    );
}

#[test]
fn unsupported_missing_and_mixed_source_offsets_reject() {
    for declaration in [
        "Irradiance Time Offset (h):-0.5, Irradiance Time Offset (h):0.5",
        "Irradiance Time Offset (h):-0.5, Irradiation Time Offset -0.25",
    ] {
        let error = parse_epw(synthetic(0, declaration).as_bytes(), Profile::PvgisTmy).unwrap_err();
        assert!(error.to_string().contains("Duplicate irradiation offset"));
    }
    for value in ["0.5", "0", "-0.25", "NaN", "-inf", "unknown"] {
        let s = synthetic(0, &format!("Irradiance Time Offset (h):{value}"));
        assert!(parse_epw(s.as_bytes(), Profile::PvgisTmy)
            .unwrap_err()
            .to_string()
            .contains("irradiance_offset"));
    }
    assert!(parse_epw(synthetic(0, "").as_bytes(), Profile::PvgisTmy).is_err());
    assert!(parse_epw(synthetic(0, "").as_bytes(), Profile::EpwHourEnding).is_err());
    rejects(
        &synthetic(60, "Irradiation Time Offset -0.5"),
        "irradiance_offset",
    );
    rejects(&mutate(&synthetic(60, ""), 1, 4, "0"), "minute");
    rejects(&mutate(&synthetic(60, ""), 1, 4, "30"), "minute");
}

#[test]
fn exact_order_rejects_duplicate_missing_reordered_and_leap_days() {
    let s = synthetic(60, "");
    rejects(&mutate(&s, 1, 3, "1"), "timestamp");
    rejects(&mutate(&s, 0, 3, "2"), "timestamp");
    rejects(&mutate(&s, 23, 3, "0"), "timestamp");
    rejects(&mutate(&s, 24, 2, "1"), "timestamp");
    let march_start = (31 + 28) * 24;
    let leap = mutate(&mutate(&s, march_start, 1, "2"), march_start, 2, "29");
    rejects(&leap, "timestamp");
    let mut lines: Vec<_> = s.lines().collect();
    lines.swap(15, 16);
    rejects(&(lines.join("\n") + "\n"), "timestamp");
    lines.remove(100);
    rejects(&(lines.join("\n") + "\n"), "records");
    rejects(&(s.clone() + s.lines().nth(8).unwrap() + "\n"), "records");
}

#[test]
fn required_fields_reject_sentinels_nonfinite_and_negative_radiation() {
    let s = synthetic(60, "");
    for column in [13, 14, 15] {
        for value in ["-0.01", "9999", "10000", "NaN", "inf", "", "x"] {
            assert!(
                parse_epw(
                    mutate(&s, 12, column, value).as_bytes(),
                    Profile::EpwHourEnding
                )
                .is_err(),
                "column={column}, value={value}"
            );
        }
    }
    for value in ["99.9", "100", "-70", "70", "NaN", ""] {
        assert!(parse_epw(mutate(&s, 0, 7, value).as_bytes(), Profile::EpwHourEnding).is_err());
    }
    // A missing field at night must fail too; it is not a zero-radiation shortcut.
    rejects(&mutate(&s, 0, 14, "9999"), "direct_normal");
}

#[test]
fn normalization_retains_original_values_and_hashes_original_bytes() {
    let s = synthetic(60, "");
    let a = parse_epw(s.as_bytes(), Profile::EpwHourEnding).unwrap();
    let crlf = s.replace('\n', "\r\n");
    let b = parse_epw(crlf.as_bytes(), Profile::EpwHourEnding).unwrap();
    assert_ne!(a.data.original_sha256, b.data.original_sha256);
    assert_eq!(a.data.normalized_sha256, b.data.normalized_sha256);
    assert_eq!(a.data.time_axis_sha256, b.data.time_axis_sha256);
    let source_rows: Vec<Vec<&str>> = s
        .lines()
        .skip(8)
        .map(|l| l.split(',').map(str::trim).collect())
        .collect();
    let normalized = String::from_utf8(a.normalized_epw).unwrap();
    for (expected, actual) in source_rows.iter().zip(normalized.lines().skip(8)) {
        assert_eq!(*expected, actual.split(',').collect::<Vec<_>>());
    }
    let changed = parse_epw(mutate(&s, 8, 14, "301").as_bytes(), Profile::EpwHourEnding).unwrap();
    assert_ne!(b.data.normalized_sha256, changed.data.normalized_sha256);
    assert_eq!(b.data.time_axis_sha256, changed.data.time_axis_sha256);
}

#[test]
fn location_bounds_longitude_sign_and_fractional_timezone_are_explicit() {
    let s = synthetic(60, "");
    for replacement in [
        ",NaN,114.17,8,18",
        ",91,114.17,8,18",
        ",22.3,181,8,18",
        ",22.3,114.17,15,18",
        ",22.3,114.17,8,-1001",
        ",22.3,114.17,8,9999.9",
        ",22.3,114.17,5.12345,18",
    ] {
        rejects(
            &s.replace(",22.3,114.17,8,18", replacement),
            "weather.line[1]",
        );
    }
    let west = parse_epw(
        s.replace(",22.3,114.17,8,18", ",22.3,-114.17,-7,18")
            .as_bytes(),
        Profile::EpwHourEnding,
    )
    .unwrap();
    assert_eq!(west.data.location.longitude_deg_east, -114.17);
    assert_eq!(west.data.interval(8759).unwrap().utc_end_minute, 526020);
    let quarter = parse_epw(
        s.replace(",114.17,8,18", ",114.17,5.75,18").as_bytes(),
        Profile::EpwHourEnding,
    )
    .unwrap();
    assert_eq!(quarter.data.location.utc_offset_minutes, 345);
    assert_eq!(
        quarter.data.interval(0).unwrap().utc_representative_minute,
        -315
    );
    assert_ne!(quarter.data.time_axis_sha256, west.data.time_axis_sha256);
}

#[test]
fn periods_headers_multiyear_leap_and_subhourly_reject() {
    let s = synthetic(60, "");
    for (from, to, error) in [
        ("DATA PERIODS,1,1", "DATA PERIODS,1,2", "data_periods"),
        ("DATA PERIODS,1,1", "DATA PERIODS,2,1", "data_periods"),
        (
            "Monday,1/1,12/31",
            "Monday,1/1/2020,12/31/2021",
            "data_period_dates",
        ),
        ("Monday,1/1,12/31", "Monday,2/1,12/31", "data_period_dates"),
        ("SAVING,No,0", "SAVING,Yes,0", "leap_year"),
        ("LOCATION,Synthetic", "BAD,Synthetic", "header"),
    ] {
        rejects(&s.replacen(from, to, 1), error);
    }
    rejects(&mutate(&s, 0, 0, "0"), "source_year");
    rejects(&mutate(&s, 0, 0, "2020.0"), "source_year");
}

#[test]
fn ignored_metadata_and_unused_quality_issues_are_visible_not_repaired() {
    let s = synthetic(60, "").replace("SAVING,No,0,0,0", "SAVING,No,3/1,10/1,1,Test holiday,1/1");
    let s = mutate(&mutate(&s, 0, 12, "-1"), 1, 21, "-2");
    let parsed = parse_epw(s.as_bytes(), Profile::EpwHourEnding).unwrap();
    for code in [
        "daylight_saving_metadata_ignored",
        "holidays_metadata_ignored",
        "unused_negative_infrared",
        "unused_invalid_wind",
    ] {
        assert_eq!(
            parsed
                .data
                .warnings
                .iter()
                .find(|w| w.code == code)
                .unwrap()
                .count,
            1
        );
    }
    assert!(String::from_utf8(parsed.normalized_epw)
        .unwrap()
        .contains(",9999,9999,-1,"));
    assert_eq!(
        parsed.data.interval(0).unwrap().utc_representative_minute,
        -450
    );
}

#[test]
fn byte_and_lexical_bounds_prevent_partial_or_nonfinite_engine_reads() {
    assert!(parse_epw(&[], Profile::EpwHourEnding).is_err());
    assert!(parse_epw(&vec![b'a'; MAX_EPW_BYTES + 1], Profile::EpwHourEnding).is_err());
    assert!(parse_epw(&[255, 254], Profile::EpwHourEnding).is_err());
    let s = synthetic(60, "");
    rejects(&s.replace("COMMENTS 1,", "COMMENTS 1,\0"), "encoding");
    rejects(&mutate(&s, 0, 34, "NaN"), "field_35");
    rejects(&mutate(&s, 0, 5, ""), "source_flags");
    rejects(&mutate(&s, 0, 5, &"a".repeat(64)), "source_flags");
    rejects(&mutate(&s, 0, 34, "1e100"), "field_35");
    rejects(&mutate(&s, 8, 14, "1e-100"), "direct_normal");
    rejects(
        &s.replace("Synthetic,Test", &format!("{},Test", "a".repeat(64))),
        "location",
    );
    rejects(
        &s.replace("000000,22.3", &format!("{},22.3", "1".repeat(16))),
        "location",
    );
    rejects(
        &s.replace("COMMENTS 2,", &format!("COMMENTS 2,{}", "a".repeat(1024))),
        "header_bytes",
    );
    rejects(
        &s.replace(",114.17,8,18", ",114.17,13,18"),
        "utc_offset_hours",
    );
}

#[test]
#[ignore = "explicit real-source acceptance; set GREENLENS_ACCEPTANCE_EPW to the reviewed original PVGIS file"]
fn reviewed_real_hong_kong_pvgis_full_year() {
    let path = std::env::var_os("GREENLENS_ACCEPTANCE_EPW")
        .expect("Required real EPW is unavailable; this is not a skipped acceptance pass");
    let original = std::fs::read(path).expect("Read required real EPW");
    let parsed = parse_epw(&original, Profile::PvgisTmy).unwrap();
    assert_eq!(
        parsed.data.original_sha256,
        "60719262a01e8827e8984537800f97f508ab4ae5ae8abc55fce571893c55b2d8"
    );
    assert_eq!(parsed.data.hours.len(), 8760);
    assert_eq!(parsed.data.location.latitude_deg_north, 22.3);
    assert_eq!(parsed.data.location.longitude_deg_east, 114.17);
    assert_eq!(parsed.data.location.utc_offset_minutes, 480);
    assert_eq!(
        parsed.data.hours[8].mean_irradiance_w_m2(),
        [865.66, 75.4, 238.8]
    );
    assert_eq!(
        parsed
            .data
            .warnings
            .iter()
            .find(|w| w.code == "unused_negative_infrared")
            .unwrap()
            .count,
        3853
    );
    assert_eq!(
        parsed
            .data
            .warnings
            .iter()
            .find(|w| w.code == "unused_invalid_wind")
            .unwrap()
            .count,
        2
    );
}
