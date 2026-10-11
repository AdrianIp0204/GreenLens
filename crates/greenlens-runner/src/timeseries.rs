//! Bounded portable climate outputs, independent of disposable engine matrices.
//! This is GreenLens's own f64 little-endian evidence format, not Radiance binary interchange.
use crate::persistence::{
    bytes_hash, read_json, regular_child, write_json, MAX_EVIDENCE_FILE_BYTES,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    canonical_hash, rgb_to_lux,
    study::{MetricAccumulator, MetricSettings, SensorMetrics, HOURS_PER_YEAR, MAX_ANNUAL_SENSORS},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const SERIES_SCHEMA: &str = "greenlens.timeseries.v0.3";
pub const CHUNK_INTERVALS: usize = 168;
const CHANNELS: usize = 4;
const BYTES_PER_SAMPLE: usize = CHANNELS * 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SeriesChunk {
    pub file: String,
    pub start: usize,
    pub intervals: usize,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimeSeriesManifest {
    pub schema_version: String,
    pub encoding: String,
    pub ordering: String,
    pub channels: Vec<String>,
    pub units: Vec<String>,
    pub sensor_ids: Vec<String>,
    pub interval_count: usize,
    pub chunks: Vec<SeriesChunk>,
}
impl TimeSeriesManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SERIES_SCHEMA,
            "Unsupported time-series version"
        );
        ensure!(
            self.encoding == "float64_little_endian" && self.ordering == "time_sensor_channel",
            "Unsupported time-series encoding/order"
        );
        ensure!(
            self.channels == ["R", "G", "B", "illuminance"]
                && self.units == ["W/m2_visible_R", "W/m2_visible_G", "W/m2_visible_B", "lx"],
            "Unsupported result channels/units"
        );
        validate_shape(&self.sensor_ids, self.interval_count)?;
        let mut next = 0;
        ensure!(
            self.chunks.len() == self.interval_count.div_ceil(CHUNK_INTERVALS),
            "Missing or excess time-series chunks"
        );
        for (index, chunk) in self.chunks.iter().enumerate() {
            ensure!(
                chunk.file == format!("data-{index:05}.f64"),
                "Unexpected time-series chunk reference"
            );
            ensure!(
                chunk.start == next
                    && chunk.intervals == (self.interval_count - next).min(CHUNK_INTERVALS),
                "Truncated or out-of-order time series"
            );
            ensure!(
                chunk.bytes == chunk.intervals * self.sensor_ids.len() * BYTES_PER_SAMPLE
                    && chunk.bytes as u64 <= MAX_EVIDENCE_FILE_BYTES,
                "Invalid chunk byte count"
            );
            ensure!(
                chunk.sha256.len() == 64
                    && chunk
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Invalid chunk SHA-256"
            );
            next += chunk.intervals;
        }
        ensure!(next == self.interval_count, "Incomplete time axis");
        Ok(())
    }
    pub fn identity(&self) -> Result<String> {
        self.validate()?;
        Ok(canonical_hash(self)?)
    }
    pub fn retained_numeric_bytes(&self) -> usize {
        self.interval_count * self.sensor_ids.len() * BYTES_PER_SAMPLE
    }
}
fn validate_shape(sensor_ids: &[String], interval_count: usize) -> Result<()> {
    ensure!(
        (1..=MAX_ANNUAL_SENSORS).contains(&sensor_ids.len()),
        "Climate results require 1–256 sensors"
    );
    ensure!(
        interval_count == 1 || interval_count == HOURS_PER_YEAR,
        "Climate result requires one selected interval or all 8,760 intervals"
    );
    let mut unique = HashSet::new();
    for id in sensor_ids {
        ensure!(
            !id.is_empty() && id.len() <= 64 && unique.insert(id),
            "Sensor IDs must be nonempty, unique and at most 64 bytes"
        );
    }
    Ok(())
}

pub struct SeriesWriter {
    root: PathBuf,
    manifest: TimeSeriesManifest,
    buffer: Vec<u8>,
    written: usize,
    flushed: usize,
}
impl SeriesWriter {
    /// Creates a new evidence subdirectory in owned scratch; never overwrites a result.
    pub fn create(root: &Path, sensor_ids: Vec<String>, interval_count: usize) -> Result<Self> {
        validate_shape(&sensor_ids, interval_count)?;
        fs::create_dir(root)
            .context("Create new time-series evidence directory; destination must not exist")?;
        Ok(Self {
            root: root.into(),
            manifest: TimeSeriesManifest {
                schema_version: SERIES_SCHEMA.into(),
                encoding: "float64_little_endian".into(),
                ordering: "time_sensor_channel".into(),
                channels: ["R", "G", "B", "illuminance"].map(String::from).to_vec(),
                units: ["W/m2_visible_R", "W/m2_visible_G", "W/m2_visible_B", "lx"]
                    .map(String::from)
                    .to_vec(),
                sensor_ids,
                interval_count,
                chunks: Vec::new(),
            },
            buffer: Vec::new(),
            written: 0,
            flushed: 0,
        })
    }
    pub fn push_hour(&mut self, rgb: &[[f64; 3]]) -> Result<()> {
        ensure!(
            self.written < self.manifest.interval_count
                && rgb.len() == self.manifest.sensor_ids.len(),
            "Unexpected result interval/sensor count"
        );
        // Validate the whole vector before any mutation, including overflow in conversion.
        let lux: Vec<f64> = rgb
            .iter()
            .map(|&v| rgb_to_lux(v))
            .collect::<greenlens_core::Result<_>>()?;
        for (values, illuminance) in rgb.iter().zip(lux) {
            for value in [values[0], values[1], values[2], illuminance] {
                self.buffer.extend_from_slice(&value.to_le_bytes());
            }
        }
        self.written += 1;
        if self.written - self.flushed == CHUNK_INTERVALS {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let file = format!("data-{:05}.f64", self.manifest.chunks.len());
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(&file))?;
        output.write_all(&self.buffer)?;
        output.sync_all()?;
        self.manifest.chunks.push(SeriesChunk {
            file,
            start: self.flushed,
            intervals: self.written - self.flushed,
            bytes: self.buffer.len(),
            sha256: bytes_hash(&self.buffer),
        });
        self.buffer.clear();
        self.flushed = self.written;
        Ok(())
    }
    pub fn finish(mut self) -> Result<TimeSeriesManifest> {
        ensure!(
            self.written == self.manifest.interval_count,
            "Partial climate run cannot be finalized"
        );
        self.flush()?;
        self.manifest.validate()?;
        write_json(&self.root.join("series.json"), &self.manifest)?;
        Ok(self.manifest)
    }
}

pub struct SeriesReader {
    root: PathBuf,
    manifest: TimeSeriesManifest,
}
impl SeriesReader {
    pub fn open(root: &Path) -> Result<Self> {
        let manifest: TimeSeriesManifest = read_json(&regular_child(root, "series.json")?)?;
        manifest.validate()?;
        Ok(Self {
            root: root.into(),
            manifest,
        })
    }
    pub fn manifest(&self) -> &TimeSeriesManifest {
        &self.manifest
    }
    fn chunk(&self, index: usize) -> Result<Vec<[f64; 4]>> {
        let entry = self
            .manifest
            .chunks
            .get(index)
            .context("Chunk index outside time axis")?;
        let path = regular_child(&self.root, &entry.file)?;
        let file = File::open(path)?;
        ensure!(
            file.metadata()?.len() == entry.bytes as u64,
            "Truncated or oversized climate chunk"
        );
        let mut bytes = Vec::with_capacity(entry.bytes + 1);
        file.take(entry.bytes as u64 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == entry.bytes && bytes_hash(&bytes) == entry.sha256,
            "Climate chunk checksum mismatch"
        );
        let mut values = Vec::with_capacity(bytes.len() / BYTES_PER_SAMPLE);
        for sample in bytes.as_chunks::<BYTES_PER_SAMPLE>().0 {
            let mut value = [0.0; 4];
            for (i, scalar) in sample.as_chunks::<8>().0.iter().enumerate() {
                value[i] = f64::from_le_bytes(*scalar);
            }
            let expected = rgb_to_lux([value[0], value[1], value[2]])?;
            ensure!(
                value[3].is_finite()
                    && value[3] >= 0.0
                    && (expected - value[3]).abs() <= 1e-9_f64.max(expected.abs() * 1e-12),
                "Stored lux is invalid or differs from raw visible RGB"
            );
            values.push(value);
        }
        Ok(values)
    }
    pub fn verify_all(&self) -> Result<()> {
        for i in 0..self.manifest.chunks.len() {
            self.chunk(i)?;
        }
        Ok(())
    }
    /// Traverse the complete retained axis with one checksum-verified bounded
    /// chunk at a time. The callback must not retain borrowed hour data.
    pub fn visit_hours(
        &self,
        mut visitor: impl FnMut(usize, &[[f64; 4]]) -> Result<()>,
    ) -> Result<()> {
        for (index, entry) in self.manifest.chunks.iter().enumerate() {
            let values = self.chunk(index)?;
            for (offset, hour) in values
                .chunks_exact(self.manifest.sensor_ids.len())
                .enumerate()
            {
                visitor(entry.start + offset, hour)?;
            }
        }
        Ok(())
    }
    pub fn hour(&self, index: usize) -> Result<Vec<f64>> {
        ensure!(
            index < self.manifest.interval_count,
            "Interval outside retained result"
        );
        let count = self.manifest.sensor_ids.len();
        let chunk = self.chunk(index / CHUNK_INTERVALS)?;
        Ok(
            chunk[(index % CHUNK_INTERVALS) * count..(index % CHUNK_INTERVALS + 1) * count]
                .iter()
                .map(|v| v[3])
                .collect(),
        )
    }
    pub fn sensor(&self, id: &str) -> Result<Vec<f64>> {
        let index = self
            .manifest
            .sensor_ids
            .iter()
            .position(|x| x == id)
            .context("Unknown sensor ID")?;
        let mut values = Vec::with_capacity(self.manifest.interval_count);
        for i in 0..self.manifest.chunks.len() {
            values.extend(
                self.chunk(i)?
                    .chunks_exact(self.manifest.sensor_ids.len())
                    .map(|hour| hour[index][3]),
            );
        }
        Ok(values)
    }
    pub fn metrics(&self, settings: &MetricSettings) -> Result<Vec<SensorMetrics>> {
        ensure!(
            self.manifest.interval_count == HOURS_PER_YEAR,
            "Annual metrics require the full 8,760-interval result"
        );
        let mut metrics = MetricAccumulator::new(self.manifest.sensor_ids.len(), settings)?;
        for i in 0..self.manifest.chunks.len() {
            for hour in self.chunk(i)?.chunks_exact(self.manifest.sensor_ids.len()) {
                metrics.push_hour(&hour.iter().map(|v| v[3]).collect::<Vec<_>>())?;
            }
        }
        Ok(metrics.finish()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{PackageBudget, MAX_PACKAGE_BYTES};
    #[test]
    fn complete_year_preserves_shape_channel_order_and_bounded_queries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("series");
        let mut writer =
            SeriesWriter::create(&root, vec!["s1".into(), "s2".into()], HOURS_PER_YEAR).unwrap();
        for hour in 0..HOURS_PER_YEAR {
            writer
                .push_hour(&[[hour as f64, 0.0, 0.0], [0.0, 1.0, 0.0]])
                .unwrap();
        }
        let manifest = writer.finish().unwrap();
        assert_eq!(manifest.chunks.len(), 53);
        let reader = SeriesReader::open(&root).unwrap();
        reader.verify_all().unwrap();
        assert_eq!(
            reader.hour(168).unwrap()[0],
            rgb_to_lux([168.0, 0.0, 0.0]).unwrap()
        );
        assert_eq!(reader.sensor("s1").unwrap().len(), 8760);
        let mut visited = 0;
        reader
            .visit_hours(|index, values| {
                assert_eq!(index, visited);
                assert_eq!(values.len(), 2);
                assert_eq!(values[0][0], index as f64);
                assert_eq!(values[0][3], rgb_to_lux([index as f64, 0.0, 0.0]).unwrap());
                visited += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(visited, HOURS_PER_YEAR);
        assert!(reader
            .visit_hours(|_, _| anyhow::bail!("visitor cancelled"))
            .is_err());
        assert!(reader.hour(8760).is_err());
        assert!(reader.sensor("missing").is_err());
        let metrics = reader.metrics(&MetricSettings::default()).unwrap();
        assert_eq!(metrics[1].udi_percent, 100.0);
        assert_eq!(metrics[1].da_percent, 0.0);
    }
    #[test]
    fn partial_invalid_and_corrupted_evidence_is_not_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("partial");
        let mut writer = SeriesWriter::create(&root, vec!["s".into()], HOURS_PER_YEAR).unwrap();
        assert!(writer.push_hour(&[[f64::NAN, 0.0, 0.0]]).is_err());
        writer.push_hour(&[[0.0; 3]]).unwrap();
        assert!(writer.finish().is_err());
        assert!(!root.join("series.json").exists());
        let root = tmp.path().join("complete");
        let mut writer = SeriesWriter::create(&root, vec!["s".into()], 1).unwrap();
        writer.push_hour(&[[1.0; 3]]).unwrap();
        let mut manifest = writer.finish().unwrap();
        let reader = SeriesReader::open(&root).unwrap();
        reader.verify_all().unwrap();
        fs::write(root.join("data-00000.f64"), vec![0u8; 31]).unwrap();
        assert!(reader.hour(0).is_err());
        fs::write(root.join("data-00000.f64"), vec![0u8; 32]).unwrap();
        assert!(reader.hour(0).is_err());
        manifest.chunks[0].file = "../outside".into();
        assert!(manifest.validate().is_err());
    }
    #[test]
    fn maximum_ab_numeric_evidence_fits_existing_archive_budget() {
        let mut budget = PackageBudget::default();
        let mut total = 0;
        for run in ["a", "b"] {
            for start in (0..HOURS_PER_YEAR).step_by(CHUNK_INTERVALS) {
                let bytes = (HOURS_PER_YEAR - start).min(CHUNK_INTERVALS)
                    * MAX_ANNUAL_SENSORS
                    * BYTES_PER_SAMPLE;
                budget
                    .add(
                        &format!("runs/{run}/series/data-{start:05}.f64"),
                        bytes as u64,
                    )
                    .unwrap();
                total += bytes as u64;
            }
        }
        assert_eq!(total, 143_523_840);
        assert!(total < MAX_PACKAGE_BYTES);
        assert!(budget
            .add("too-large", MAX_EVIDENCE_FILE_BYTES + 1)
            .is_err());
    }
    #[test]
    fn resealed_nonfinite_negative_or_inconsistent_channels_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("series");
        let mut writer = SeriesWriter::create(&root, vec!["s".into()], 1).unwrap();
        writer.push_hour(&[[0.0; 3]]).unwrap();
        let mut manifest = writer.finish().unwrap();
        for values in [
            [0.0, 0.0, 0.0, -1e-10],
            [0.0, 0.0, 0.0, f64::NAN],
            [0.0, 0.0, 0.0, 1.0],
            [f64::INFINITY, 0.0, 0.0, 0.0],
        ] {
            let bytes: Vec<u8> = values.into_iter().flat_map(f64::to_le_bytes).collect();
            fs::write(root.join("data-00000.f64"), &bytes).unwrap();
            manifest.chunks[0].sha256 = bytes_hash(&bytes);
            write_json(&root.join("series.json"), &manifest).unwrap();
            assert!(SeriesReader::open(&root).unwrap().verify_all().is_err());
        }
    }
}
