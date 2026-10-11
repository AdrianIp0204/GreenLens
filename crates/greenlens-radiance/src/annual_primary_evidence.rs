//! Portable evidence for the compiled primary generator. No process APIs here.
use super::{ENCODING_VERSION, GENERATOR_VERSION, MAX_RECORD_BYTES, MAX_SENSORS, PRIMARY_COUNTS};
use crate::{process, ProcessRecord};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

pub const PRIMARY_STREAMS_SCHEMA_VERSION: &str = "greenlens.annual-primary-streams.v1";
pub const STREAM_RESOURCE_SCHEMA_VERSION: &str = "greenlens.stream-resources.v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExternalPrimaryContract {
    pub generator_version: String,
    pub encoding_version: String,
    pub count: usize,
}
impl ExternalPrimaryContract {
    pub fn new(count: usize) -> Result<Self> {
        let value = Self {
            generator_version: GENERATOR_VERSION.into(),
            encoding_version: ENCODING_VERSION.into(),
            count,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.generator_version == GENERATOR_VERSION
                && self.encoding_version == ENCODING_VERSION
                && PRIMARY_COUNTS.contains(&self.count),
            "Unsupported external-primary contract"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InputReceipt {
    pub bytes: u64,
    pub records: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamTelemetry {
    pub schema_version: String,
    pub sampled_peak_job_plus_runner_working_set_bytes: u64,
    pub peak_child_working_set_bytes: u64,
    pub sampled_peak_runner_plus_child_working_set_bytes: u64,
    pub peak_scratch_bytes: u64,
    pub samples: u64,
    pub zero_progress_writes: u64,
    pub partial_writes: u64,
    pub atomic_job_assignment: bool,
}
impl StreamTelemetry {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == STREAM_RESOURCE_SCHEMA_VERSION
                && self.atomic_job_assignment
                && self.samples > 0,
            "Unsupported or incomplete stream resource evidence"
        );
        ensure!(
            self.peak_child_working_set_bytes > 0
                && self.peak_child_working_set_bytes
                    <= process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES
                && self.sampled_peak_runner_plus_child_working_set_bytes > 0
                && self.sampled_peak_runner_plus_child_working_set_bytes
                    <= process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES
                && self.sampled_peak_job_plus_runner_working_set_bytes > 0
                && self.sampled_peak_job_plus_runner_working_set_bytes
                    <= process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES
                && self.peak_scratch_bytes <= process::ANNUAL_MAX_SCRATCH_BYTES,
            "Invalid stream memory or scratch measurements"
        );
        // Child and Job queries occur at adjacent times. Do not invent an exact
        // ordering between sampled peaks or confuse lifetime with sampled peaks.
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryPurpose {
    Full,
    Direct,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryStreamRecord {
    pub purpose: PrimaryPurpose,
    pub process_index: usize,
    pub input: InputReceipt,
    pub matrix_sha256: String,
    pub resources: StreamTelemetry,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryStreams {
    pub schema_version: String,
    pub contract: ExternalPrimaryContract,
    pub sensor_sha256: String,
    pub sensor_count: usize,
    pub sky_rows: usize,
    pub records: Vec<PrimaryStreamRecord>,
}
fn hash_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl PrimaryStreams {
    #[allow(clippy::too_many_arguments)]
    pub fn validate(
        &self,
        contract: &ExternalPrimaryContract,
        sensor_count: usize,
        sensor_sha256: &str,
        sky_rows: usize,
        has_suns: bool,
        processes: &[ProcessRecord],
    ) -> Result<()> {
        contract.validate()?;
        ensure!(
            self.schema_version == PRIMARY_STREAMS_SCHEMA_VERSION
                && &self.contract == contract
                && self.sensor_count == sensor_count
                && (1..=MAX_SENSORS).contains(&sensor_count)
                && self.sensor_sha256 == sensor_sha256
                && hash_valid(sensor_sha256)
                && self.sky_rows == sky_rows
                && [578, 2306].contains(&sky_rows),
            "Primary stream contract or matrix identity mismatch"
        );
        ensure!(
            self.records.len() == if has_suns { 2 } else { 1 },
            "Unexpected primary stream record count"
        );
        let expected = (sensor_count as u64)
            .checked_mul(contract.count as u64)
            .ok_or_else(|| anyhow::anyhow!("Primary count overflow"))?;
        for (index, record) in self.records.iter().enumerate() {
            let (purpose, process_index) = if index == 0 {
                (PrimaryPurpose::Full, 4)
            } else {
                (PrimaryPurpose::Direct, 6)
            };
            ensure!(
                record.purpose == purpose && record.process_index == process_index,
                "Primary stream purpose or process order mismatch"
            );
            ensure!(
                record.input.records == expected
                    && record.input.bytes >= expected * 132
                    && record.input.bytes <= expected * MAX_RECORD_BYTES as u64
                    && hash_valid(&record.input.sha256)
                    && hash_valid(&record.matrix_sha256),
                "Invalid primary input receipt or matrix hash"
            );
            record.resources.validate()?;
            let process = processes
                .get(process_index)
                .ok_or_else(|| anyhow::anyhow!("Missing primary process"))?;
            ensure!(
                process.executable == "rcontrib.exe" && process.exit_code == 0,
                "Primary stream process did not complete"
            );
        }
        if has_suns {
            ensure!(
                self.records[0].input == self.records[1].input,
                "Paired primary receipts differ"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Explicitly synthetic metadata, only wire/validation behavior is tested.
    fn fixture() -> (PrimaryStreams, Vec<ProcessRecord>) {
        let contract = ExternalPrimaryContract::new(16_384).unwrap();
        let records = [PrimaryPurpose::Full, PrimaryPurpose::Direct]
            .into_iter()
            .enumerate()
            .map(|(index, purpose)| PrimaryStreamRecord {
                purpose,
                process_index: if index == 0 { 4 } else { 6 },
                input: InputReceipt {
                    bytes: 16_384 * 140,
                    records: 16_384,
                    sha256: "0".repeat(64),
                },
                matrix_sha256: "1".repeat(64),
                resources: StreamTelemetry {
                    schema_version: STREAM_RESOURCE_SCHEMA_VERSION.into(),
                    sampled_peak_job_plus_runner_working_set_bytes: 3,
                    peak_child_working_set_bytes: 1,
                    sampled_peak_runner_plus_child_working_set_bytes: 2,
                    peak_scratch_bytes: 4,
                    samples: 1,
                    zero_progress_writes: 0,
                    partial_writes: 0,
                    atomic_job_assignment: true,
                },
            })
            .collect();
        let streams = PrimaryStreams {
            schema_version: PRIMARY_STREAMS_SCHEMA_VERSION.into(),
            contract,
            sensor_sha256: "2".repeat(64),
            sensor_count: 1,
            sky_rows: 578,
            records,
        };
        let process = ProcessRecord {
            executable: "rcontrib.exe".into(),
            arguments: vec![],
            exit_code: 0,
            started_utc: "2026-01-01T00:00:00Z".into(),
            finished_utc: "2026-01-01T00:00:00Z".into(),
            elapsed_ms: 0,
            stderr: String::new(),
        };
        (streams, vec![process; 7])
    }
    fn check(value: &PrimaryStreams, processes: &[ProcessRecord], suns: bool) -> Result<()> {
        value.validate(
            &ExternalPrimaryContract::new(16_384)?,
            1,
            &"2".repeat(64),
            578,
            suns,
            processes,
        )
    }
    #[test]
    fn paired_receipts_bind_shape_order_purpose_resources_and_complete_input() {
        let (good, processes) = fixture();
        check(&good, &processes, true).unwrap();
        let mut value = good.clone();
        value.records[0].input.bytes = 1;
        assert!(check(&value, &processes, true).is_err());
        let mut value = good.clone();
        value.records[0].input.bytes = 16_384 * 193;
        assert!(check(&value, &processes, true).is_err());
        let mut value = good.clone();
        value.records[1].input.sha256 = "3".repeat(64);
        assert!(check(&value, &processes, true).is_err());
        let mut value = good.clone();
        value.records.swap(0, 1);
        assert!(check(&value, &processes, true).is_err());
        let mut value = good.clone();
        value.records[1].process_index = 5;
        assert!(check(&value, &processes, true).is_err());
        let mut value = good.clone();
        value.sensor_sha256 = "3".repeat(64);
        assert!(check(&value, &processes, true).is_err());
        let mut value = good.clone();
        value.records[0].resources.schema_version = "greenlens.stream-resources.v1".into();
        assert!(check(&value, &processes, true).is_err());
        let mut value = good.clone();
        value.records[0].resources.atomic_job_assignment = false;
        assert!(check(&value, &processes, true).is_err());
        let mut failed = processes.clone();
        failed[4].exit_code = 1;
        assert!(check(&good, &failed, true).is_err());
        let mut no_sun = good;
        no_sun.records.pop();
        check(&no_sun, &processes, false).unwrap();
        assert!(check(&no_sun, &processes, true).is_err());
    }
    #[test]
    fn evidence_contract_rejects_unknown_fields_and_generator_aliases() {
        let (good, _) = fixture();
        let mut json = serde_json::to_value(&good).unwrap();
        json["records"][0]["resources"]["unreported_memory"] = serde_json::json!(1);
        assert!(serde_json::from_value::<PrimaryStreams>(json).is_err());
        let mut contract = good.contract;
        contract.encoding_version = "C#G17".into();
        assert!(contract.validate().is_err());
        assert!(ExternalPrimaryContract::new(16_385).is_err());
    }
}
