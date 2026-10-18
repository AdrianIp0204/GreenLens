//! Independent thermal inputs and energy accounting. No engine object mapping,
//! execution or scientific qualification is implied by accepting these inputs.
pub mod accounting;
pub mod comparison;
pub mod design_alternatives;
pub mod design_figures;
pub mod diagnostics;
pub mod example;
pub mod investigation;
pub mod model;
pub mod report;
pub mod report_workbench;
pub mod scene;
pub mod schedule;
pub mod sensitivity;
pub mod study;
pub mod weather;
pub mod workbench;

pub use accounting::*;
pub use comparison::*;
pub use design_alternatives::*;
pub use design_figures::*;
pub use diagnostics::*;
pub use example::*;
pub use investigation::*;
pub use model::*;
pub use report::*;
pub use report_workbench::*;
pub use scene::*;
pub use schedule::*;
pub use sensitivity::*;
pub use study::*;
pub use workbench::*;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod weather_policy_tests;

use crate::{CoreError, Result};

pub const THERMAL_MODEL_SCHEMA: &str = "greenlens.thermal-model.v0.4";
pub const ENERGY_STUDY_SCHEMA: &str = "greenlens.energy-study.v0.4";
pub const ENERGY_REPORT_SCHEMA: &str = "greenlens.energy-report.v0.4";
pub const ENERGY_INTERVALS: usize = 52_560;
pub const ENERGY_INTERVAL_SECONDS: u32 = 600;
pub const MAX_THERMAL_ZONES: usize = 8;

pub(crate) fn invalid(field: &str, message: impl Into<String>) -> CoreError {
    CoreError::Validation {
        field: field.into(),
        message: message.into(),
    }
}
pub(crate) fn finite(field: &str, value: f64) -> Result<()> {
    if !value.is_finite() {
        return Err(invalid(field, "Expected a finite number"));
    }
    Ok(())
}
pub(crate) fn range(field: &str, value: f64, low: f64, high: f64) -> Result<()> {
    finite(field, value)?;
    if !(low..=high).contains(&value) {
        return Err(invalid(field, format!("Expected {low} through {high}")));
    }
    Ok(())
}
pub(crate) fn positive(field: &str, value: f64) -> Result<()> {
    finite(field, value)?;
    if value <= 0. {
        return Err(invalid(field, "Expected a positive number"));
    }
    Ok(())
}
pub(crate) fn identifier(field: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(invalid(
            field,
            "Expected 1–80 ASCII letters, digits, underscores or hyphens",
        ));
    }
    Ok(())
}
pub(crate) fn text(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > 4096
        || value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(invalid(
            field,
            "Expected bounded nonempty text without control characters",
        ));
    }
    Ok(())
}
pub(crate) fn hash(field: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(field, "Expected a lowercase SHA-256"));
    }
    Ok(())
}

/// Neumaier accumulation; totals retain small contributions without changing
/// the published unit conversion or comparison tolerance.
#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct Sum {
    value: f64,
    correction: f64,
}
impl Sum {
    pub(crate) fn add(&mut self, value: f64) {
        let next = self.value + value;
        self.correction += if self.value.abs() >= value.abs() {
            (self.value - next) + value
        } else {
            (value - next) + self.value
        };
        self.value = next;
    }
    pub(crate) fn value(self) -> f64 {
        self.value + self.correction
    }
}
