//! Portable native-output representation. Engine CSV is parsed into bounded,
//! checksummed float64 chunks; warm-up and sizing rows never enter this series.
use crate::persistence::{bytes_hash, read_json, regular_child, write_json};
use anyhow::{ensure, Context, Result};
use greenlens_core::canonical_hash;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const SCHEMA: &str = "greenlens.energy-series.v0.4";
pub const MAX_CHANNELS: usize = 64;
pub const CHUNK_HOURS: usize = 168;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyChannel {
    pub id: String,
    pub label: String,
    pub unit: String,
}
impl EnergyChannel {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 120
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b)),
            "Invalid energy channel ID"
        );
        ensure!(
            !self.label.is_empty()
                && self.label.len() <= 240
                && !self.label.chars().any(char::is_control),
            "Invalid energy channel label"
        );
        ensure!(
            ["J", "C", "fraction", "W", "m3/s"].contains(&self.unit.as_str()),
            "Unsupported energy series unit"
        );
        Ok(())
    }
    fn validate_value(&self, value: f64) -> Result<()> {
        ensure!(value.is_finite(), "Nonfinite output in channel {}", self.id);
        match self.unit.as_str() {
            "J" | "W" | "m3/s" => ensure!(value >= 0.0, "Negative output in channel {}", self.id),
            "fraction" => ensure!(
                (0.0..=1.0).contains(&value),
                "Fraction outside [0,1] in {}",
                self.id
            ),
            "C" => ensure!(
                (-150.0..=200.0).contains(&value),
                "Temperature outside supported range in {}",
                self.id
            ),
            _ => anyhow::bail!("Unsupported channel unit"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyChunk {
    pub file: String,
    pub start: usize,
    pub intervals: usize,
    pub bytes: usize,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySeriesManifest {
    pub schema_version: String,
    pub encoding: String,
    pub ordering: String,
    pub time_policy: String,
    pub interval_minutes: u16,
    pub interval_count: usize,
    pub channels: Vec<EnergyChannel>,
    pub chunks: Vec<EnergyChunk>,
}
fn validate_shape(channels: &[EnergyChannel], minutes: u16) -> Result<usize> {
    ensure!(
        [5, 10].contains(&minutes),
        "Energy intervals must be 10 minutes or the 5-minute sensitivity profile"
    );
    ensure!(
        (1..=MAX_CHANNELS).contains(&channels.len()),
        "Energy output requires 1–64 declared channels"
    );
    let mut ids = HashSet::new();
    for channel in channels {
        channel.validate()?;
        ensure!(ids.insert(&channel.id), "Duplicate energy channel ID");
    }
    Ok(8760 * 60 / minutes as usize)
}
impl EnergySeriesManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA
                && self.encoding == "float64_little_endian"
                && self.ordering == "time_channel"
                && self.time_policy
                    == "non_leap_local_standard_time_interval_end_excludes_warmup_sizing",
            "Unsupported energy series contract"
        );
        ensure!(
            self.interval_count == validate_shape(&self.channels, self.interval_minutes)?,
            "Incomplete energy weather-run year"
        );
        let size = CHUNK_HOURS * 60 / self.interval_minutes as usize;
        ensure!(
            self.chunks.len() == self.interval_count.div_ceil(size),
            "Missing/excess energy chunks"
        );
        let mut next = 0;
        for (index, chunk) in self.chunks.iter().enumerate() {
            ensure!(
                chunk.file == format!("data-{index:05}.f64")
                    && chunk.start == next
                    && chunk.intervals == size.min(self.interval_count - next),
                "Invalid energy chunk path/order/shape"
            );
            ensure!(
                chunk.bytes == chunk.intervals * self.channels.len() * 8,
                "Incorrect energy chunk size"
            );
            ensure!(
                chunk.sha256.len() == 64
                    && chunk
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Invalid energy chunk digest"
            );
            next += chunk.intervals;
        }
        ensure!(next == self.interval_count, "Incomplete energy time axis");
        Ok(())
    }
    pub fn identity(&self) -> Result<String> {
        self.validate()?;
        Ok(canonical_hash(self)?)
    }
    pub fn retained_numeric_bytes(&self) -> usize {
        self.interval_count * self.channels.len() * 8
    }
}

pub struct EnergySeriesWriter {
    root: PathBuf,
    manifest: EnergySeriesManifest,
    buffer: Vec<u8>,
    written: usize,
    flushed: usize,
}
impl EnergySeriesWriter {
    pub fn create(
        root: &Path,
        channels: Vec<EnergyChannel>,
        interval_minutes: u16,
    ) -> Result<Self> {
        let interval_count = validate_shape(&channels, interval_minutes)?;
        fs::create_dir(root).context("Create new energy series; destination must not exist")?;
        Ok(Self {
            root: root.into(),
            manifest: EnergySeriesManifest {
                schema_version: SCHEMA.into(),
                encoding: "float64_little_endian".into(),
                ordering: "time_channel".into(),
                time_policy: "non_leap_local_standard_time_interval_end_excludes_warmup_sizing"
                    .into(),
                interval_minutes,
                interval_count,
                channels,
                chunks: vec![],
            },
            buffer: vec![],
            written: 0,
            flushed: 0,
        })
    }
    pub fn push(&mut self, index: usize, values: &[f64]) -> Result<()> {
        ensure!(
            index == self.written
                && self.written < self.manifest.interval_count
                && values.len() == self.manifest.channels.len(),
            "Energy output order/shape differs from declared year"
        );
        for (channel, &value) in self.manifest.channels.iter().zip(values) {
            channel.validate_value(value)?;
        }
        for value in values {
            self.buffer.extend_from_slice(&value.to_le_bytes());
        }
        self.written += 1;
        if self.written - self.flushed == CHUNK_HOURS * 60 / self.manifest.interval_minutes as usize
        {
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
        self.manifest.chunks.push(EnergyChunk {
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
    pub fn finish(mut self) -> Result<EnergySeriesManifest> {
        ensure!(
            self.written == self.manifest.interval_count,
            "Partial energy run cannot be finalized"
        );
        self.flush()?;
        self.manifest.validate()?;
        write_json(&self.root.join("series.json"), &self.manifest)?;
        Ok(self.manifest)
    }
}
pub struct EnergySeriesReader {
    root: PathBuf,
    manifest: EnergySeriesManifest,
}
impl EnergySeriesReader {
    pub fn open(root: &Path) -> Result<Self> {
        let manifest: EnergySeriesManifest = read_json(&regular_child(root, "series.json")?)?;
        manifest.validate()?;
        Ok(Self {
            root: root.into(),
            manifest,
        })
    }
    pub fn manifest(&self) -> &EnergySeriesManifest {
        &self.manifest
    }
    fn chunk(&self, index: usize) -> Result<Vec<f64>> {
        let entry = self
            .manifest
            .chunks
            .get(index)
            .context("Energy chunk outside retained axis")?;
        let file = File::open(regular_child(&self.root, &entry.file)?)?;
        ensure!(
            file.metadata()?.len() == entry.bytes as u64,
            "Truncated/oversized energy chunk"
        );
        let mut bytes = Vec::with_capacity(entry.bytes + 1);
        file.take(entry.bytes as u64 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == entry.bytes && bytes_hash(&bytes) == entry.sha256,
            "Energy chunk checksum mismatch"
        );
        let mut values = Vec::with_capacity(entry.bytes / 8);
        for (i, scalar) in bytes.as_chunks::<8>().0.iter().enumerate() {
            let value = f64::from_le_bytes(*scalar);
            self.manifest.channels[i % self.manifest.channels.len()].validate_value(value)?;
            values.push(value);
        }
        Ok(values)
    }
    pub fn visit(&self, mut visitor: impl FnMut(usize, &[f64]) -> Result<()>) -> Result<()> {
        for (index, chunk) in self.manifest.chunks.iter().enumerate() {
            for (offset, row) in self
                .chunk(index)?
                .chunks_exact(self.manifest.channels.len())
                .enumerate()
            {
                visitor(chunk.start + offset, row)?;
            }
        }
        Ok(())
    }
    pub fn verify_all(&self) -> Result<()> {
        self.visit(|_, _| Ok(()))
    }
    /// Bounded IPC query; no full-year matrix or unlimited DOM table.
    pub fn rows(&self, start: usize, count: usize) -> Result<Vec<Vec<f64>>> {
        ensure!(
            (1..=2016).contains(&count)
                && start < self.manifest.interval_count
                && count <= self.manifest.interval_count - start,
            "Request 1–2016 intervals within the retained year"
        );
        let chunk_rows = CHUNK_HOURS * 60 / self.manifest.interval_minutes as usize;
        let mut output = Vec::with_capacity(count);
        for c in start / chunk_rows..=(start + count - 1) / chunk_rows {
            let entry = &self.manifest.chunks[c];
            for (offset, row) in self
                .chunk(c)?
                .chunks_exact(self.manifest.channels.len())
                .enumerate()
            {
                let i = entry.start + offset;
                if (start..start + count).contains(&i) {
                    output.push(row.to_vec());
                }
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn channels() -> Vec<EnergyChannel> {
        vec![
            EnergyChannel {
                id: "facility".into(),
                label: "Facility electricity".into(),
                unit: "J".into(),
            },
            EnergyChannel {
                id: "zone.temperature".into(),
                label: "Zone temperature".into(),
                unit: "C".into(),
            },
        ]
    }
    fn full(root: &Path) -> EnergySeriesManifest {
        let mut writer = EnergySeriesWriter::create(root, channels(), 10).unwrap();
        for i in 0..52560 {
            writer.push(i, &[i as f64, 24.0]).unwrap();
        }
        writer.finish().unwrap()
    }
    #[test]
    fn complete_year_roundtrip_and_queries_preserve_every_ordered_value() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("series");
        let manifest = full(&root);
        assert_eq!(manifest.chunks.len(), 53);
        let reader = EnergySeriesReader::open(&root).unwrap();
        let mut count = 0;
        reader
            .visit(|i, row| {
                assert_eq!(row, &[i as f64, 24.0]);
                count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count, 52560);
        assert_eq!(
            reader.rows(1007, 3).unwrap(),
            vec![vec![1007.0, 24.0], vec![1008.0, 24.0], vec![1009.0, 24.0]]
        );
        assert!(reader.rows(0, 52560).is_err());
        assert!(reader.rows(52559, 2).is_err());
    }
    #[test]
    fn invalid_cancelled_or_partial_stream_never_finalizes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("series");
        let mut w = EnergySeriesWriter::create(&root, channels(), 10).unwrap();
        assert!(w.push(1, &[1.0, 24.0]).is_err());
        assert!(w.push(0, &[-1.0, 24.0]).is_err());
        assert!(w.push(0, &[f64::NAN, 24.0]).is_err());
        assert!(w.push(0, &[1.0]).is_err());
        w.push(0, &[1.0, 24.0]).unwrap();
        assert!(w.finish().is_err());
        assert!(!root.join("series.json").exists());
    }
    #[test]
    fn tampered_truncated_reordered_and_resealed_nonfinite_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("series");
        let mut m = full(&root);
        let path = root.join(&m.chunks[0].file);
        let original = fs::read(&path).unwrap();
        fs::write(&path, &original[..8]).unwrap();
        assert!(EnergySeriesReader::open(&root)
            .unwrap()
            .verify_all()
            .is_err());
        let mut bytes = original;
        bytes[..8].copy_from_slice(&f64::INFINITY.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        m.chunks[0].sha256 = bytes_hash(&bytes);
        write_json(&root.join("series.json"), &m).unwrap();
        assert!(EnergySeriesReader::open(&root)
            .unwrap()
            .verify_all()
            .is_err());
        m.chunks[0].start = 1;
        assert!(m.validate().is_err());
        m.chunks[0].start = 0;
        m.chunks[0].file = "../escape".into();
        assert!(m.validate().is_err());
    }
    #[test]
    fn maximum_shape_is_bounded_by_shared_package_policy() {
        let mut budget = crate::persistence::PackageBudget::default();
        for alternative in ["A", "B"] {
            for chunk in 0..53 {
                budget
                    .add(
                        &format!("runs/{alternative}/series/data-{chunk:05}.f64"),
                        (CHUNK_HOURS * 6 * MAX_CHANNELS * 8) as u64,
                    )
                    .unwrap();
            }
        }
        assert!(validate_shape(&channels(), 15).is_err());
        let mut duplicate = channels();
        duplicate.push(duplicate[0].clone());
        assert!(validate_shape(&duplicate, 10).is_err());
    }
}
