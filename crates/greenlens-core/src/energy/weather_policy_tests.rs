//! Admission-only tests. Synthetic tokens are not measured weather or engine
//! acceptance; the adapter separately guards restricted consumers.
use super::weather::*;
use super::EnergyWeatherBinding;

fn changed(rows: &[(usize, usize, &str)]) -> Vec<u8> {
    let source = String::from_utf8(synthetic_energy_epw()).unwrap();
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    for &(hour, column, token) in rows {
        let mut fields: Vec<String> = lines[8 + hour].split(',').map(str::to_owned).collect();
        fields[column] = token.into();
        lines[8 + hour] = fields.join(",");
    }
    (lines.join("\n") + "\n").into_bytes()
}

#[test]
fn strict_binding_is_exact_historical_identity_and_policy_selection_is_explicit() {
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../examples/energy/revision1-inputs.json"
    ))
    .unwrap();
    let expected: EnergyWeatherBinding =
        serde_json::from_value(frozen["cases"]["A"]["study"]["weather"].clone()).unwrap();
    let bytes = synthetic_energy_epw();
    let strict = parse_energy_epw(&bytes).unwrap();
    let explicit =
        parse_energy_epw_with_profile(&bytes, EnergyWeatherProfile::ExplicitFieldsV1).unwrap();
    assert_eq!(strict.data.binding(), expected);
    assert_eq!(strict.data, explicit.data);
    assert_eq!(strict.normalized_epw, explicit.normalized_epw);
    let restricted =
        parse_energy_epw_with_profile(&bytes, EnergyWeatherProfile::DryEnvelopeV1).unwrap();
    assert_eq!(strict.normalized_epw, restricted.normalized_epw);
    assert_eq!(strict.data.hours, restricted.data.hours);
    assert_eq!(strict.data.original_sha256, restricted.data.original_sha256);
    assert_eq!(
        strict.data.normalized_sha256,
        restricted.data.normalized_sha256
    );
    assert_ne!(
        strict.data.time_axis_sha256,
        restricted.data.time_axis_sha256
    );
    assert_ne!(strict.data.profile_id, restricted.data.profile_id);
    assert!(EnergyWeatherProfile::from_id("greenlens.energy-epw-dry-envelope.v2").is_err());
    let mut unknown = expected;
    unknown.profile_id = "greenlens.energy-epw-unreviewed.v1".into();
    assert!(unknown.validate().is_err());
}

#[test]
fn missing_unused_values_retain_tokens_indices_counts_and_strict_rejection() {
    let changes = [
        (0, 32, "999"),
        (744, 32, "999"),
        (744, 33, "999"),
        (8759, 33, "999"),
    ];
    let bytes = changed(&changes);
    assert!(parse_energy_epw(&bytes).is_err());
    let parsed =
        parse_energy_epw_with_profile(&bytes, EnergyWeatherProfile::DryEnvelopeV1).unwrap();
    let normalized = String::from_utf8(parsed.normalized_epw).unwrap();
    let rows: Vec<Vec<&str>> = normalized
        .lines()
        .skip(8)
        .map(|r| r.split(',').collect())
        .collect();
    for &(hour, column, token) in &changes {
        assert_eq!(rows[hour][column], token);
    }
    let albedo: Vec<_> = parsed
        .data
        .hours
        .iter()
        .filter(|h| h.albedo == 999.)
        .map(|h| h.index)
        .collect();
    let rain: Vec<_> = parsed
        .data
        .hours
        .iter()
        .filter(|h| h.liquid_precipitation_depth_mm == 999.)
        .map(|h| h.index)
        .collect();
    assert_eq!(albedo, [0, 744]);
    assert_eq!(rain, [744, 8759]);
    for code in [
        "energy_unused_missing_albedo",
        "energy_unused_missing_liquid_precipitation",
    ] {
        let warning = parsed
            .data
            .warnings
            .iter()
            .find(|w| w.code == code)
            .unwrap();
        assert_eq!(warning.count, 2);
        assert!(warning.message.contains("not zero or observations"));
    }
    assert_eq!(parsed.data.hours[1].albedo, 0.2);
    assert_eq!(parsed.data.hours[1].liquid_precipitation_depth_mm, 0.);
}

#[test]
fn restricted_policy_only_excepting_the_two_exact_sentinels() {
    for (column, token) in [
        (32, "-0.01"),
        (32, "1.01"),
        (32, "998.999"),
        (32, "999.0001"),
        (33, "-0.01"),
        (33, "999.0001"),
        (33, "1000"),
        (32, "NaN"),
        (32, "inf"),
        (33, "NaN"),
        (33, "-inf"),
        (9, "0"),
        (9, "31000"),
        (12, "-1"),
        (12, "9999"),
        (21, "-1"),
        (21, "999"),
        (30, "-1"),
        (30, "999"),
        (8, "101"),
        (22, "99"),
    ] {
        let bytes = changed(&[(0, 32, "999"), (0, 33, "999"), (0, column, token)]);
        assert!(
            parse_energy_epw_with_profile(&bytes, EnergyWeatherProfile::DryEnvelopeV1).is_err(),
            "column {column}: {token}"
        );
    }
    // Zero albedo/precipitation are valid explicit values, not missingness.
    let bytes = changed(&[(0, 32, "0"), (0, 33, "0")]);
    for policy in [
        EnergyWeatherProfile::ExplicitFieldsV1,
        EnergyWeatherProfile::DryEnvelopeV1,
    ] {
        let data = parse_energy_epw_with_profile(&bytes, policy).unwrap().data;
        assert_eq!(data.hours[0].albedo, 0.);
        assert_eq!(data.hours[0].liquid_precipitation_depth_mm, 0.);
        assert!(!data
            .warnings
            .iter()
            .any(|w| w.code.starts_with("energy_unused_missing_")));
    }
}

#[test]
fn dry_profile_preserves_location_chronology_and_remaining_field_rules() {
    for edits in [
        vec![(0, 7, "60")],               // Dew point above the original dry bulb.
        vec![(0, 22, "0"), (0, 23, "1")], // Opaque exceeds total cover.
        vec![(0, 2, "2"), (0, 3, "29")],  // Non-leap chronology.
        vec![(0, 4, "0")],                // EPW hour-ending remains 1..24.
    ] {
        let bytes = changed(&edits);
        assert!(
            parse_energy_epw_with_profile(&bytes, EnergyWeatherProfile::DryEnvelopeV1).is_err()
        );
    }
}
