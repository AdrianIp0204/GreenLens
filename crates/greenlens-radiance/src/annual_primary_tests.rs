//! Pure generation/encoding tests, not engine or annual accuracy acceptance.
use super::*;
use sha2::{Digest, Sha256};

fn sensor(id: &str, position: Vec3) -> Sensor {
    Sensor {
        id: id.into(),
        position,
        direction: [0.0, 0.0, 1.0],
    }
}
fn values(record: &[u8]) -> [f64; 6] {
    assert!(record.is_ascii() && record.ends_with(b"\n") && !record.contains(&b'\r'));
    assert!(record.len() <= MAX_RECORD_BYTES);
    let fields: Vec<_> = std::str::from_utf8(record)
        .unwrap()
        .trim_end_matches('\n')
        .split(' ')
        .collect();
    assert_eq!(fields.len(), 6);
    let mut result = [0.0; 6];
    for (index, field) in fields.iter().enumerate() {
        let (mantissa, exponent) = field.split_once('e').unwrap();
        assert_eq!(mantissa.split_once('.').unwrap().1.len(), 17);
        assert!(!exponent.starts_with('+'));
        assert_eq!(exponent.parse::<i32>().unwrap().to_string(), *exponent);
        result[index] = field.parse::<f64>().unwrap();
        assert!(result[index].is_finite());
    }
    result
}

#[test]
fn known_base2_values_and_quadrant_directions_are_independent() {
    for (index, expected) in [0.0, 0.5, 0.25, 0.75, 0.125, 0.625, 0.375, 0.875]
        .into_iter()
        .enumerate()
    {
        assert_eq!(radical_inverse_base2(index as u32), expected);
    }
    assert_eq!(radical_inverse_base2(13), 0.6875);
    assert_eq!(radical_inverse_base2(262_143), 262_143.0 / 262_144.0);
    let input = [sensor("s000001", [0.0, 0.0, 0.8])];
    let rays = HammersleyPrimary::new(&input, 16_384).unwrap();
    for (index, (ray, axis)) in rays
        .take(4)
        .zip([[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]])
        .enumerate()
    {
        let v = values(&ray.unwrap());
        let radial_length = ((index as f64 + 0.5) / 16_384.0).sqrt();
        assert!((v[3] - axis[0] * radial_length).abs() < 3e-18);
        assert!((v[4] - axis[1] * radial_length).abs() < 3e-18);
    }
}

#[test]
fn malformed_sensor_inputs_and_unsealed_counts_are_rejected() {
    let valid = sensor("s000001", [1.0, 2.0, 0.8]);
    for count in [0, 1, 32_768, usize::MAX] {
        assert!(HammersleyPrimary::new(std::slice::from_ref(&valid), count).is_err());
    }
    assert!(HammersleyPrimary::new(&[], 16_384).is_err());
    assert!(HammersleyPrimary::new(&vec![valid.clone(); 257], 16_384).is_err());
    assert!(HammersleyPrimary::new(&[valid.clone(), valid.clone()], 16_384).is_err());
    for id in [String::new(), "x".repeat(65), "line\nbreak".into()] {
        let mut bad = valid.clone();
        bad.id = id;
        assert!(HammersleyPrimary::new(&[bad], 16_384).is_err());
    }
    for coordinate in 0..3 {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut bad = valid.clone();
            bad.position[coordinate] = invalid;
            assert!(HammersleyPrimary::new(&[bad], 16_384).is_err());
        }
    }
    for direction in [
        [0.0, 1.0, 0.0],
        [0.0, 0.0, -1.0],
        [0.0, 1e-15, 1.0],
        [0.0, 0.0, f64::NAN],
    ] {
        let mut bad = valid.clone();
        bad.direction = direction;
        assert!(HammersleyPrimary::new(&[bad], 16_384).is_err());
    }
    let mut bad = valid;
    bad.position[2] = f64::MAX;
    assert!(HammersleyPrimary::new(&[bad], 16_384).is_err());
}

#[test]
fn maximum_shape_is_lazy_and_every_supported_count_has_finite_cosine_rays() {
    let sensors: Vec<_> = (0..256)
        .map(|i| sensor(&format!("s{i:06}"), [i as f64, 1.0, 0.8]))
        .collect();
    let mut maximum = HammersleyPrimary::new(&sensors, 262_144).unwrap();
    assert_eq!(maximum.origins.len(), 256);
    assert_eq!(maximum.expected_records(), 67_108_864);
    assert_eq!(maximum.size_hint(), (0, Some(67_108_864)));
    assert!(maximum.next().unwrap().is_ok());
    assert_eq!(maximum.emitted, 1);
    #[cfg(windows)]
    {
        const { assert!(MAX_RECORD_BYTES <= crate::annual_stream::MAX_RECORD_BYTES) };
        assert!(maximum.expected_records() <= crate::annual_stream::MAX_RECORDS);
    }
    for count in PRIMARY_COUNTS {
        let mut iterator = HammersleyPrimary::new(&sensors[..1], count).unwrap();
        for index in 0..count {
            let v = values(&iterator.next().unwrap().unwrap());
            assert_eq!(&v[..3], &[0.0, 1.0, 0.8 + ORIGIN_OFFSET_M]);
            assert!(v[5] > 0.0);
            let horizontal = v[3] * v[3] + v[4] * v[4];
            assert!((horizontal + v[5] * v[5] - 1.0).abs() < 2e-15);
            assert!((horizontal - (index as f64 + 0.5) / count as f64).abs() < 2e-15);
        }
        assert!(iterator.next().is_none());
        assert!(iterator.next().is_none());
    }
}

#[test]
fn exact_ascii_encoding_has_independent_golden_prefix_and_hash() {
    // Independently encoded from the frozen research producer's parsed doubles
    // using .NET e17 with canonical integer exponent; not C# G17 byte equality.
    let expected = concat!(
        "1.00000000000000000e0 2.00000000000000000e0 8.00100000000000033e-1 5.52427172801990291e-3 0.00000000000000000e0 9.99984741094520402e-1\n",
        "1.00000000000000000e0 2.00000000000000000e0 8.00100000000000033e-1 -9.56831930774678860e-3 1.17178116134519231e-18 9.99954222585026642e-1\n",
        "1.00000000000000000e0 2.00000000000000000e0 8.00100000000000033e-1 7.56381487214919421e-19 1.23526471100327325e-2 9.99923703144082410e-1\n",
        "1.00000000000000000e0 2.00000000000000000e0 8.00100000000000033e-1 -2.68488793489380298e-18 -1.46158491670857083e-2 9.99893182771602329e-1\n",
    );
    let actual: Vec<_> = HammersleyPrimary::new(&[sensor("s000001", [1.0, 2.0, 0.8])], 16_384)
        .unwrap()
        .take(4)
        .flat_map(|v| v.unwrap())
        .collect();
    assert_eq!(actual, expected.as_bytes());
    assert_eq!(
        format!("{:x}", Sha256::digest(&actual)),
        "3986153617703d769ea37659a7e66de5f79e9a082fb31277571066bb426fb3e7"
    );
    assert_eq!(ENCODING_VERSION, "greenlens.primary-ascii-e17-lf.v1");
    assert_eq!(GENERATOR_VERSION, "greenlens.hammersley-cosine-primary.v1");
    let signed =
        HammersleyPrimary::new(&[sensor("signed", [-0.0, f64::MIN_POSITIVE, 0.8])], 16_384)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
    assert!(std::str::from_utf8(&signed)
        .unwrap()
        .starts_with("-0.00000000000000000e0 2.22507385850720138e-308 "));
}

#[test]
fn full_paired_stream_regeneration_preserves_order_counts_and_golden_digest() {
    let sensors = [
        sensor("s000001", [1.0, 2.0, 0.8]),
        sensor("s000002", [-3.0, 4.0, 1.2]),
    ];
    let mut full = HammersleyPrimary::new(&sensors, 16_384).unwrap();
    let mut direct = HammersleyPrimary::new(&sensors, 16_384).unwrap();
    assert_eq!(
        (
            full.sensor_count(),
            full.primary_count(),
            full.expected_records()
        ),
        (2, 16_384, 32_768)
    );
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    for index in 0..32_768 {
        let a = full.next().unwrap().unwrap();
        let b = direct.next().unwrap().unwrap();
        assert_eq!(a, b);
        let v = values(&a);
        let sensor_index = index / 16_384;
        assert_eq!(
            &v[..3],
            &[
                sensors[sensor_index].position[0],
                sensors[sensor_index].position[1],
                sensors[sensor_index].position[2] + ORIGIN_OFFSET_M
            ]
        );
        if index == 16_384 {
            assert_eq!(v[3], 0.005_524_271_728_019_903);
            assert_eq!(v[4], 0.0);
        }
        digest.update(&a);
        bytes += a.len() as u64;
    }
    assert!(full.next().is_none() && direct.next().is_none());
    assert_eq!(bytes, 4_489_218);
    assert_eq!(
        format!("{:x}", digest.finalize()),
        "7dd8b4a77fede4c7e9030c015dff5003916fd9ba09f99c01d06f86808914b895"
    );
}

#[test]
// Retain the decimal spellings captured from the independently executed G17
// research producer, including digits beyond Rust's shortest f64 spelling.
#[allow(clippy::excessive_precision)]
fn scalar_values_match_frozen_research_generation_at_rounding_boundaries() {
    // Actual GenerateAsync records captured from the research C# helper SHA256
    // 49ccb427479ee2951c1b94753dafecd318b519511c3aca7c6b629eed695045cd.
    // Numerical cross-check only, predeclared absolute component bound 2e-15.
    // Near-axis, bit-order boundaries and near-horizon cancellation are included.
    let cases: &[(usize, usize, [f64; 3])] = &[
        (
            16_384,
            1,
            [
                -0.0095683193077467886,
                1.1717811613451923e-18,
                0.99995422258502664,
            ],
        ),
        (
            16_384,
            2,
            [
                7.5638148721491942e-19,
                0.012352647110032733,
                0.99992370314408241,
            ],
        ),
        (
            16_384,
            7,
            [
                0.015128841196122719,
                -0.015128841196122726,
                0.99977109196461822,
            ],
        ),
        (
            16_384,
            127,
            [
                0.088109285751720043,
                -0.0043285316444198188,
                0.99610140928427815,
            ],
        ),
        (
            16_384,
            128,
            [
                0.088534140019806903,
                0.0021733911470849326,
                0.99607077179378922,
            ],
        ),
        (
            16_384,
            8191,
            [
                0.70708499369082445,
                -0.00054232750420767377,
                0.70712836004372293,
            ],
        ),
        (
            16_384,
            8192,
            [
                0.70712830804554661,
                0.00027118032307202084,
                0.70708520167082767,
            ],
        ),
        (
            16_384,
            16382,
            [
                -0.99995414905411073,
                0.00038347763215299457,
                0.0095683193077467886,
            ],
        ),
        (
            16_384,
            16383,
            [
                0.9999846675613604,
                -0.0003834893358550838,
                0.0055242717280199029,
            ],
        ),
        (65_536, 0, [0.0027621358640099515, 0.0, 0.99999618529545842]),
        (
            65_536,
            32768,
            [
                0.70711217271276594,
                6.7793530696555541e-5,
                0.70710138636935849,
            ],
        ),
        (
            65_536,
            65534,
            [
                -0.99998855124687869,
                9.5872701901107352e-5,
                0.0047841596538733943,
            ],
        ),
        (
            65_536,
            65535,
            [
                0.99999618069958329,
                -9.5873433365848979e-5,
                0.0027621358639898543,
            ],
        ),
        (
            262_144,
            0,
            [0.0013810679320049757, 0.0, 0.99999904632522885],
        ),
        (
            262_144,
            8192,
            [
                0.17678207701138665,
                6.779508076800521e-5,
                0.98425001531692502,
            ],
        ),
        (
            262_144,
            131072,
            [
                0.70710812968130154,
                1.6948285720259038e-5,
                0.70710543248610891,
            ],
        ),
        (
            262_144,
            262142,
            [
                -0.99999713868571549,
                2.3968381234118515e-5,
                0.0023920798269366971,
            ],
        ),
        (
            262_144,
            262143,
            [
                0.99999904603798573,
                -2.3968426950518147e-5,
                0.0013810679320049757,
            ],
        ),
    ];
    for count in PRIMARY_COUNTS {
        let selected: Vec<_> = cases.iter().filter(|(n, _, _)| *n == count).collect();
        let mut position = 0;
        for (index, record) in HammersleyPrimary::new(&[sensor("s000001", [1.0, 2.0, 0.8])], count)
            .unwrap()
            .enumerate()
        {
            if position == selected.len() {
                break;
            }
            if index == selected[position].1 {
                let actual = values(&record.unwrap());
                for channel in 0..3 {
                    assert!(
                        (actual[channel + 3] - selected[position].2[channel]).abs() <= 2e-15,
                        "count {count} index {index} channel {channel}"
                    );
                }
                position += 1;
            }
        }
        assert_eq!(position, selected.len());
    }
}
