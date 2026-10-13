//! Portable thermal-weather admission, independent of daylight suitability.
use crate::{
    persistence::{read_json, regular_child, seal, verify_seal, write_json, WriterLock},
    weather::{read_original, WeatherSource},
};
use anyhow::{ensure, Result};
use greenlens_core::{
    canonical_hash,
    energy::{
        weather::{parse_energy_epw, EnergyWeatherData, ParsedEnergyWeather, ENERGY_EPW_PROFILE},
        EnergyWeatherBinding,
    },
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use uuid::Uuid;

pub const ASSET_SCHEMA: &str = "greenlens.energy-weather-asset.v0.4";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyWeatherAsset {
    pub schema_version: String,
    pub binding: EnergyWeatherBinding,
    pub source: WeatherSource,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyWeatherInspection {
    pub binding: EnergyWeatherBinding,
    pub location: greenlens_core::weather::WeatherLocation,
    pub interval_count: usize,
    pub warnings: Vec<greenlens_core::weather::WeatherWarning>,
    pub interpretation: String,
}
impl From<&EnergyWeatherData> for EnergyWeatherInspection {
    fn from(data: &EnergyWeatherData) -> Self {
        Self{binding:data.binding(),location:data.location.clone(),interval_count:data.hours.len(),warnings:data.warnings.clone(),interpretation:"Hourly interval-end local standard time, non-leap synthetic calendar explicitly chosen in study; no operating-system timezone or daylight saving. Radiation is hourly accumulation in Wh/m². Required thermal fields are explicit; no zero substitution or weather-field derivation.".into()}
    }
}
pub fn inspect(path: &Path) -> Result<EnergyWeatherInspection> {
    Ok(EnergyWeatherInspection::from(
        &parse_energy_epw(&read_original(path)?)?.data,
    ))
}

pub fn import(
    path: &Path,
    destination: &Path,
    source: WeatherSource,
) -> Result<EnergyWeatherInspection> {
    source.validate()?;
    let original = read_original(path)?;
    let parsed = parse_energy_epw(&original)?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let _lock = WriterLock::acquire(parent)?;
    ensure!(
        fs::symlink_metadata(destination).is_err(),
        "Energy weather destination already exists; choose a new directory"
    );
    let stage = parent.join(format!(".energy-weather-pending-{}", Uuid::new_v4()));
    fs::create_dir(&stage)?;
    fs::write(stage.join("original.epw"), original)?;
    fs::write(stage.join("normalized.epw"), &parsed.normalized_epw)?;
    write_json(&stage.join("weather.json"), &parsed.data)?;
    write_json(
        &stage.join("asset.json"),
        &EnergyWeatherAsset {
            schema_version: ASSET_SCHEMA.into(),
            binding: parsed.data.binding(),
            source,
        },
    )?;
    seal(
        &stage,
        &[
            "asset.json",
            "weather.json",
            "original.epw",
            "normalized.epw",
        ],
    )?;
    open(&stage)?;
    fs::rename(&stage, destination)?;
    Ok(EnergyWeatherInspection::from(&parsed.data))
}
pub fn open(root: &Path) -> Result<(EnergyWeatherAsset, ParsedEnergyWeather)> {
    let checksums = verify_seal(root)?;
    ensure!(
        checksums.sha256.keys().map(String::as_str).eq([
            "asset.json",
            "normalized.epw",
            "original.epw",
            "weather.json"
        ]),
        "Energy weather evidence coverage mismatch"
    );
    let asset: EnergyWeatherAsset = read_json(&regular_child(root, "asset.json")?)?;
    ensure!(
        asset.schema_version == ASSET_SCHEMA && asset.binding.profile_id == ENERGY_EPW_PROFILE,
        "Unsupported energy weather asset or provider profile"
    );
    asset.binding.validate()?;
    asset.source.validate()?;
    let parsed = parse_energy_epw(&read_original(&regular_child(root, "original.epw")?)?)?;
    ensure!(
        asset.binding == parsed.data.binding(),
        "Energy weather binding differs from retained original bytes"
    );
    let retained: EnergyWeatherData = read_json(&regular_child(root, "weather.json")?)?;
    ensure!(
        canonical_hash(&retained)? == canonical_hash(&parsed.data)?,
        "Energy weather metadata differs from independent reparse"
    );
    ensure!(
        read_original(&regular_child(root, "normalized.epw")?)? == parsed.normalized_epw,
        "Energy normalized weather differs from provider policy"
    );
    Ok((asset, parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> WeatherSource {
        WeatherSource {
            citation: "GreenLens original non-climatic thermal fixture".into(),
            source_url: None,
            license: "Apache-2.0".into(),
            rights_note: "Original synthetic data".into(),
            synthetic_non_climatic: true,
        }
    }
    #[test]
    fn original_portability_and_reparse_prevent_forged_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let epw = tmp.path().join("original.epw");
        let asset = tmp.path().join("asset");
        fs::write(
            &epw,
            greenlens_core::energy::weather::synthetic_energy_epw(),
        )
        .unwrap();
        let info = import(&epw, &asset, source()).unwrap();
        assert_eq!(info.interval_count, 8760);
        fs::remove_file(epw).unwrap();
        let (a, _) = open(&asset).unwrap();
        assert_eq!(a.binding, info.binding);
        let mut data: EnergyWeatherData = read_json(&asset.join("weather.json")).unwrap();
        data.hours[0].dry_bulb_c += 1.;
        write_json(&asset.join("weather.json"), &data).unwrap();
        seal(
            &asset,
            &[
                "asset.json",
                "weather.json",
                "original.epw",
                "normalized.epw",
            ],
        )
        .unwrap();
        assert!(open(&asset).is_err());
    }
    #[test]
    fn thermal_rejection_does_not_modify_source_or_create_asset() {
        let tmp = tempfile::tempdir().unwrap();
        let epw = tmp.path().join("bad.epw");
        let asset = tmp.path().join("asset");
        let text =
            String::from_utf8(greenlens_core::energy::weather::synthetic_energy_epw()).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut row: Vec<_> = lines[8].split(',').map(str::to_owned).collect();
        row[12] = "-1".into();
        lines[8] = row.join(",");
        let bytes = lines.join("\n").into_bytes();
        fs::write(&epw, &bytes).unwrap();
        assert!(import(&epw, &asset, source()).is_err());
        assert!(!asset.exists());
        assert_eq!(fs::read(&epw).unwrap(), bytes);
    }
}
