//! Local data-provider services shared by CLI and desktop; no downloader required.
use crate::persistence::{read_json, regular_child, seal, verify_seal, write_json, WriterLock};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    canonical_hash,
    study::WeatherBinding,
    weather::{
        parse_epw, ParsedWeather, WeatherData, WeatherLocation, WeatherSourceProfile,
        WeatherSummary, WeatherTimePolicy, WeatherWarning, MAX_EPW_BYTES,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WeatherSource {
    pub citation: String,
    pub source_url: Option<String>,
    pub license: String,
    pub rights_note: String,
    pub synthetic_non_climatic: bool,
}
impl WeatherSource {
    pub fn validate(&self) -> Result<()> {
        for (name, text) in [
            ("citation", &self.citation),
            ("license", &self.license),
            ("rights note", &self.rights_note),
        ] {
            ensure!(!text.trim().is_empty() && text.len() <= 4000, "Weather {name} must contain 1–4,000 bytes; use an explicit unknown-rights note if necessary");
        }
        if let Some(url) = &self.source_url {
            ensure!(
                url.len() <= 4000 && (url.starts_with("https://") || url.starts_with("http://")),
                "Weather source URL must be an HTTP(S) citation, not an imported local path"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherInspection {
    pub binding: WeatherBinding,
    pub location: WeatherLocation,
    pub time_policy: WeatherTimePolicy,
    pub summary: WeatherSummary,
    pub warnings: Vec<WeatherWarning>,
}
impl From<&WeatherData> for WeatherInspection {
    fn from(data: &WeatherData) -> Self {
        Self {
            binding: data.binding(),
            location: data.location.clone(),
            time_policy: data.time_policy.clone(),
            summary: data.summary.clone(),
            warnings: data.warnings.clone(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WeatherAsset {
    pub schema_version: String,
    pub binding: WeatherBinding,
    pub profile: WeatherSourceProfile,
    pub source: WeatherSource,
}

pub fn read_original(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).context("Read local weather file")?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= MAX_EPW_BYTES as u64,
        "EPW must be a regular file no larger than 16 MiB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_EPW_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_EPW_BYTES, "EPW exceeds 16 MiB");
    Ok(bytes)
}
pub fn inspect(path: &Path, profile: WeatherSourceProfile) -> Result<WeatherInspection> {
    Ok(WeatherInspection::from(
        &parse_epw(&read_original(path)?, profile)?.data,
    ))
}

/// Import into a new portable asset directory, publishing only after verification.
pub fn import(
    path: &Path,
    destination: &Path,
    profile: WeatherSourceProfile,
    source: WeatherSource,
) -> Result<WeatherInspection> {
    source.validate()?;
    let original = read_original(path)?;
    let parsed = parse_epw(&original, profile)?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let _lock = WriterLock::acquire(parent)?;
    ensure!(
        fs::symlink_metadata(destination).is_err(),
        "Weather destination already exists; choose a new directory"
    );
    let stage = parent.join(format!(".weather-pending-{}", Uuid::new_v4()));
    fs::create_dir(&stage)?;
    fs::write(stage.join("original.epw"), original)?;
    fs::write(stage.join("normalized.epw"), &parsed.normalized_epw)?;
    write_json(&stage.join("weather.json"), &parsed.data)?;
    write_json(
        &stage.join("asset.json"),
        &WeatherAsset {
            schema_version: "greenlens.weather-asset.v0.3".into(),
            binding: parsed.data.binding(),
            profile,
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
    fs::rename(&stage, destination)
        .context("Publish verified weather asset; destination must remain unused")?;
    Ok(WeatherInspection::from(&parsed.data))
}

/// Reparse retained original bytes. A self-consistent forged metadata JSON is not validation.
pub fn open(root: &Path) -> Result<(WeatherAsset, ParsedWeather)> {
    let seal = verify_seal(root)?;
    let expected = [
        "asset.json",
        "normalized.epw",
        "original.epw",
        "weather.json",
    ];
    ensure!(
        seal.sha256.keys().map(String::as_str).eq(expected),
        "Weather asset checksum coverage differs from the required four files"
    );
    let asset: WeatherAsset = read_json(&regular_child(root, "asset.json")?)?;
    ensure!(
        asset.schema_version == "greenlens.weather-asset.v0.3",
        "Unsupported weather asset version"
    );
    asset.source.validate()?;
    let parsed = parse_epw(
        &read_original(&regular_child(root, "original.epw")?)?,
        asset.profile,
    )?;
    ensure!(
        asset.binding == parsed.data.binding(),
        "Weather identity differs from retained original bytes"
    );
    let stored: WeatherData = read_json(&regular_child(root, "weather.json")?)?;
    ensure!(
        canonical_hash(&stored)? == canonical_hash(&parsed.data)?,
        "Weather metadata differs from validated original data"
    );
    ensure!(
        read_original(&regular_child(root, "normalized.epw")?)? == parsed.normalized_epw,
        "Normalized weather differs from documented source policy"
    );
    Ok((asset, parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::weather::{synthetic_epw, WEATHER_HOURS};
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn synthetic_source() -> WeatherSource {
        WeatherSource {
            citation: "GreenLens original repeated-day test fixture; not climatic data".into(),
            source_url: None,
            license: "Apache-2.0".into(),
            rights_note:
                "Original synthetic test data, freely redistributable with the project license"
                    .into(),
            synthetic_non_climatic: true,
        }
    }

    fn import_fixture(root: &Path) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
        let source = root.join("synthetic source.epw");
        let asset = root.join("weather asset");
        fs::write(&source, synthetic_epw())?;
        import(
            &source,
            &asset,
            WeatherSourceProfile::EpwHourEnding,
            synthetic_source(),
        )?;
        Ok((source, asset))
    }

    fn file_bytes(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut files = BTreeMap::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            ensure!(entry.file_type()?.is_file(), "Unexpected fixture directory");
            files.insert(
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path())?,
            );
        }
        Ok(files)
    }

    fn reseal_fixture(root: &Path) -> Result<()> {
        seal(
            root,
            &[
                "asset.json",
                "weather.json",
                "original.epw",
                "normalized.epw",
            ],
        )
    }

    #[test]
    fn portable_weather_reopens_without_import_source_or_original_asset() -> Result<()> {
        let original_root = TempDir::new()?;
        let (source, asset) = import_fixture(original_root.path())?;
        let expected = inspect(&source, WeatherSourceProfile::EpwHourEnding)?;
        let expected_files = file_bytes(&asset)?;
        let relocated_root = TempDir::new()?;
        let relocated = relocated_root.path().join("copied weather 空間");
        fs::create_dir(&relocated)?;
        for (name, bytes) in &expected_files {
            fs::write(relocated.join(name), bytes)?;
        }
        // Close the owned temporary directory so neither the import path nor its
        // original portable asset is available to the reader.
        original_root.close()?;
        assert!(!source.exists());
        assert!(!asset.exists());

        let (stored, parsed) = open(&relocated)?;
        assert_eq!(stored.source, synthetic_source());
        assert_eq!(stored.binding, expected.binding);
        assert_eq!(parsed.data.hours.len(), WEATHER_HOURS);
        assert_eq!(
            canonical_hash(&WeatherInspection::from(&parsed.data))?,
            canonical_hash(&expected)?
        );
        assert_eq!(fs::read(relocated.join("original.epw"))?, synthetic_epw());
        assert_eq!(file_bytes(&relocated)?, expected_files);
        Ok(())
    }

    #[test]
    fn resealed_forged_weather_metadata_is_rejected_by_reparsing() -> Result<()> {
        let root = TempDir::new()?;
        let (_, asset) = import_fixture(root.path())?;
        let mut stored: WeatherData = read_json(&asset.join("weather.json"))?;
        stored.hours[0].direct_normal_wh_m2 += 1.0;
        write_json(&asset.join("weather.json"), &stored)?;
        reseal_fixture(&asset)?;
        verify_seal(&asset)?;

        let error = open(&asset).expect_err("Forged weather must not open");
        assert!(error
            .to_string()
            .contains("metadata differs from validated original"));
        Ok(())
    }

    #[test]
    fn resealed_forged_binding_is_rejected() -> Result<()> {
        let root = TempDir::new()?;
        let (_, asset) = import_fixture(root.path())?;
        let mut stored: WeatherAsset = read_json(&asset.join("asset.json"))?;
        stored.binding.original_sha256 = "0".repeat(64);
        write_json(&asset.join("asset.json"), &stored)?;
        reseal_fixture(&asset)?;
        verify_seal(&asset)?;

        let error = open(&asset).expect_err("Forged identity must not open");
        assert!(error
            .to_string()
            .contains("identity differs from retained original"));
        Ok(())
    }

    #[test]
    fn resealed_altered_normalized_weather_is_rejected() -> Result<()> {
        let root = TempDir::new()?;
        let (_, asset) = import_fixture(root.path())?;
        let mut normalized = fs::read(asset.join("normalized.epw"))?;
        normalized.push(b'\n');
        fs::write(asset.join("normalized.epw"), normalized)?;
        reseal_fixture(&asset)?;
        verify_seal(&asset)?;

        let error = open(&asset).expect_err("Altered normalization must not open");
        assert!(error.to_string().contains("Normalized weather differs"));
        Ok(())
    }

    #[test]
    fn invalid_citations_and_local_source_paths_reject_on_import_and_open() -> Result<()> {
        let root = TempDir::new()?;
        let (input, asset) = import_fixture(root.path())?;
        let original: WeatherAsset = read_json(&asset.join("asset.json"))?;
        let mut invalid_sources = Vec::new();
        let mut empty_citation = synthetic_source();
        empty_citation.citation = " \t ".into();
        invalid_sources.push(empty_citation);
        let mut excessive_citation = synthetic_source();
        excessive_citation.citation = "x".repeat(4001);
        invalid_sources.push(excessive_citation);
        for invalid_url in [
            "file:///C:/private/weather.epw",
            "C:\\private\\weather.epw",
            "../weather.epw",
        ] {
            let mut source = synthetic_source();
            source.source_url = Some(invalid_url.into());
            invalid_sources.push(source);
        }

        for (index, source) in invalid_sources.into_iter().enumerate() {
            let destination = root.path().join(format!("rejected-{index}"));
            let error = import(
                &input,
                &destination,
                WeatherSourceProfile::EpwHourEnding,
                source.clone(),
            )
            .expect_err("Invalid source must fail import");
            assert!(error.to_string().contains("Weather"));
            assert!(!destination.exists());

            let mut forged = original.clone();
            forged.source = source;
            write_json(&asset.join("asset.json"), &forged)?;
            reseal_fixture(&asset)?;
            verify_seal(&asset)?;
            let error = open(&asset).expect_err("Invalid source must fail reopen");
            assert!(error.to_string().contains("Weather"));
        }
        Ok(())
    }

    #[test]
    fn import_never_overwrites_existing_file_directory_or_weather_asset() -> Result<()> {
        let root = TempDir::new()?;
        let (input, asset) = import_fixture(root.path())?;
        let original_asset = file_bytes(&asset)?;
        let existing_file = root.path().join("existing file");
        fs::write(&existing_file, b"preserve existing file")?;
        let existing_directory = root.path().join("existing directory");
        fs::create_dir(&existing_directory)?;
        fs::write(
            existing_directory.join("keep.txt"),
            b"preserve existing directory",
        )?;
        let original_directory = file_bytes(&existing_directory)?;

        for destination in [&existing_file, &existing_directory, &asset] {
            let error = import(
                &input,
                destination,
                WeatherSourceProfile::EpwHourEnding,
                synthetic_source(),
            )
            .expect_err("Existing destination must reject import");
            assert!(error.to_string().contains("destination already exists"));
        }
        assert_eq!(fs::read(existing_file)?, b"preserve existing file");
        assert_eq!(file_bytes(&existing_directory)?, original_directory);
        assert_eq!(file_bytes(&asset)?, original_asset);
        open(&asset)?;
        Ok(())
    }
}
