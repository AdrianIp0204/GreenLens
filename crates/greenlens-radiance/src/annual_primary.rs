//! Bounded external primary generator used by the compiled method-5 adapter.
//!
//! The mathematical rule matches the declared research Hammersley experiment.
//! This module owns a distinct versioned Rust encoding, not C# G17 byte identity.
//! Memory is bounded by 256 origins and one short ASCII record. It performs no
//! I/O, culling, process launch, sensor resampling, scrambling or random seeding.

use anyhow::{ensure, Context, Result};
use greenlens_core::{Sensor, Vec3};
use std::{collections::BTreeSet, iter::FusedIterator};

#[path = "annual_primary_evidence.rs"]
mod evidence;
pub use evidence::*;

pub const GENERATOR_VERSION: &str = "greenlens.hammersley-cosine-primary.v1";
pub const ENCODING_VERSION: &str = "greenlens.primary-ascii-e17-lf.v1";
pub const ENCODING_POLICY: &str = "Six finite f64 values per ray: origin XYZ then direction XYZ; ASCII lowercase scientific notation with 17 digits after the decimal point (18 significant digits), exponent without a plus sign or redundant leading zeros; one space between values, one LF after each ray, no BOM. Signed zero is preserved.";
pub const RULE: &str = "Sensor-major, primary index i=0..N-1 at each sensor: r=sqrt((i+0.5)/N), phi=2*pi*radical_inverse_base2(i), direction=(r*cos(phi),r*sin(phi),sqrt(1-r*r)); no rotation, scramble or culling. World-up sensors only; origin=sensor.position+(0,0,0.0001)m. One uninterrupted stream; no native process restart between sensors.";
pub const PRIMARY_COUNTS: [usize; 3] = [16_384, 65_536, 262_144];
pub const MAX_SENSORS: usize = 256;
pub const MAX_RECORD_BYTES: usize = 192;
pub const ORIGIN_OFFSET_M: f64 = 0.0001;

/// Validated, bounded iterator compatible with annual_stream::execute.
/// Cloning preserves its current position; call `new` for a fresh paired stream.
#[derive(Clone, Debug)]
pub struct HammersleyPrimary {
    origins: Vec<Vec3>,
    primary_count: usize,
    expected_records: usize,
    emitted: usize,
}

impl HammersleyPrimary {
    pub fn new(sensors: &[Sensor], primary_count: usize) -> Result<Self> {
        ensure!(
            PRIMARY_COUNTS.contains(&primary_count),
            "Unsupported Hammersley primary count"
        );
        ensure!(
            (1..=MAX_SENSORS).contains(&sensors.len()),
            "Hammersley input requires 1 through 256 sensors"
        );
        let expected_records = sensors
            .len()
            .checked_mul(primary_count)
            .context("Primary record count overflow")?;
        let mut ids = BTreeSet::new();
        let mut origins = Vec::with_capacity(sensors.len());
        for sensor in sensors {
            ensure!(
                !sensor.id.is_empty()
                    && sensor.id.len() <= 64
                    && !sensor.id.chars().any(char::is_control)
                    && ids.insert(sensor.id.as_str()),
                "Sensor IDs must be nonempty, bounded and unique"
            );
            ensure!(
                sensor
                    .position
                    .iter()
                    .chain(&sensor.direction)
                    .all(|v| v.is_finite()),
                "Primary sensor coordinates and directions must be finite"
            );
            ensure!(
                sensor.direction == [0.0, 0.0, 1.0],
                "Hammersley primary sensors must point exactly world-up"
            );
            let origin = [
                sensor.position[0],
                sensor.position[1],
                sensor.position[2] + ORIGIN_OFFSET_M,
            ];
            ensure!(
                origin[2].is_finite() && origin[2] > sensor.position[2],
                "Effective primary origin offset is not representable"
            );
            origins.push(origin);
        }
        Ok(Self {
            origins,
            primary_count,
            expected_records,
            emitted: 0,
        })
    }

    pub fn sensor_count(&self) -> usize {
        self.origins.len()
    }
    pub fn primary_count(&self) -> usize {
        self.primary_count
    }
    pub fn expected_records(&self) -> u64 {
        self.expected_records as u64
    }

    fn next_record(&mut self) -> Result<Vec<u8>> {
        let sensor = self.emitted / self.primary_count;
        let index = self.emitted % self.primary_count;
        let radius = ((index as f64 + 0.5) / self.primary_count as f64).sqrt();
        let phi = 2.0 * std::f64::consts::PI * radical_inverse_base2(index as u32);
        // Separate operations preserve the declared research arithmetic. Do not
        // replace this with z=sqrt(1-(i+.5)/N), FMA, or normalization silently.
        let direction = [
            radius * phi.cos(),
            radius * phi.sin(),
            (1.0 - radius * radius).sqrt(),
        ];
        ensure!(
            direction.iter().all(|v| v.is_finite()) && direction[2] > 0.0,
            "Invalid generated primary direction"
        );
        let origin = self.origins[sensor];
        let line = format!(
            "{:.17e} {:.17e} {:.17e} {:.17e} {:.17e} {:.17e}\n",
            origin[0], origin[1], origin[2], direction[0], direction[1], direction[2]
        );
        ensure!(
            line.len() <= MAX_RECORD_BYTES && line.is_ascii(),
            "Primary ASCII record exceeds its encoding bound"
        );
        self.emitted += 1;
        Ok(line.into_bytes())
    }
}

fn radical_inverse_base2(index: u32) -> f64 {
    // Exact binary fraction for the supported <=18-bit indices. This provides
    // an implementation distinct from the research loop over successive bits.
    f64::from(index.reverse_bits()) / 4_294_967_296.0
}

impl Iterator for HammersleyPrimary {
    type Item = Result<Vec<u8>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.emitted == self.expected_records {
            return None;
        }
        let result = self.next_record();
        if result.is_err() {
            self.emitted = self.expected_records;
        }
        Some(result)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        // A numerical/encoding fault terminates the iterator early, so do not
        // advertise an ExactSizeIterator contract for an error path.
        (0, Some(self.expected_records - self.emitted))
    }
}
impl FusedIterator for HammersleyPrimary {}

#[cfg(test)]
#[path = "annual_primary_tests.rs"]
mod tests;

#[cfg(all(test, windows))]
#[path = "annual_primary_native_tests.rs"]
mod native_tests;
