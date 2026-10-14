//! Versioned presentation supplement; the v0.4 report codec stays immutable.
use super::*;
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const ENERGY_WORKBENCH_REPORT_SCHEMA: &str = "greenlens.energy-workbench-report.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnergyReportProfile {
    Executive,
    Technical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportRevision {
    pub revision: String,
    pub issue_date: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportMetadata {
    pub title: String,
    pub revision: String,
    /// None is disclosed as not supplied; never invent an issue date or author.
    pub issue_date: Option<String>,
    pub author: Option<String>,
    pub preparer: Option<String>,
    pub revision_history: Vec<EnergyReportRevision>,
}
impl Default for EnergyReportMetadata {
    fn default() -> Self {
        Self {
            title: "Energy assessment".into(),
            revision: "Draft".into(),
            issue_date: None,
            author: None,
            preparer: None,
            revision_history: vec![],
        }
    }
}
fn short(field: &str, s: &str, bound: usize) -> Result<()> {
    text(field, s)?;
    if s.len() > bound {
        return Err(invalid(field, "Report metadata exceeds its byte bound"));
    }
    Ok(())
}
fn date(s: &str) -> Result<()> {
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || b.iter()
            .enumerate()
            .any(|(i, c)| i != 4 && i != 7 && !c.is_ascii_digit())
    {
        return Err(invalid(
            "report_metadata.issue_date",
            "Expected an explicit YYYY-MM-DD calendar date",
        ));
    }
    let y = s[0..4]
        .parse::<u32>()
        .map_err(|_| invalid("report_metadata.issue_date", "Invalid year"))?;
    let m = s[5..7]
        .parse::<usize>()
        .map_err(|_| invalid("report_metadata.issue_date", "Invalid month"))?;
    let d = s[8..10]
        .parse::<u32>()
        .map_err(|_| invalid("report_metadata.issue_date", "Invalid day"))?;
    let days = [
        31,
        if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if y == 0 || !(1..=12).contains(&m) || d == 0 || d > days[m - 1] {
        return Err(invalid(
            "report_metadata.issue_date",
            "Invalid calendar date",
        ));
    }
    Ok(())
}
impl EnergyReportMetadata {
    pub fn validate(&self) -> Result<()> {
        short("report_metadata.title", &self.title, 256)?;
        short("report_metadata.revision", &self.revision, 80)?;
        if let Some(d) = &self.issue_date {
            date(d)?;
        }
        for s in [&self.author, &self.preparer].into_iter().flatten() {
            short("report_metadata.person", s, 256)?;
        }
        if self.revision_history.len() > 32 {
            return Err(invalid(
                "report_metadata.revisions",
                "At most 32 explicit revision records",
            ));
        }
        for r in &self.revision_history {
            short("report_metadata.revision", &r.revision, 80)?;
            date(&r.issue_date)?;
            short("report_metadata.description", &r.description, 2048)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyAttachmentDisposition {
    Included,
    Excluded,
    Missing,
    Inapplicable,
    Invalid,
}
/// Local pre-export inspection only. Excluded private names/paths never enter
/// the public report supplement; the runner supplies a count-only summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyAttachmentDecision {
    pub id: String,
    pub selected: bool,
    pub disposition: EnergyAttachmentDisposition,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyAttachmentPreflight {
    pub schema_version: String,
    pub ready: bool,
    pub items: Vec<EnergyAttachmentDecision>,
}
impl EnergyAttachmentPreflight {
    pub fn summary(&self) -> Result<EnergyReportAttachmentSummary> {
        if self.schema_version != "greenlens.energy-attachment-preflight.v1"
            || self.items.len() > 1024
        {
            return Err(invalid(
                "report.attachments",
                "Invalid bounded preflight schema",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut included = vec![];
        let mut excluded = 0;
        let mut missing = 0;
        let mut inapplicable = 0;
        let mut expected_ready = true;
        for item in &self.items {
            text("report.attachment.id", &item.id)?;
            text("report.attachment.reason", &item.reason)?;
            if !ids.insert(&item.id) {
                return Err(invalid(
                    "report.attachments",
                    "Duplicate attachment identity",
                ));
            }
            if item.selected {
                if item.disposition == EnergyAttachmentDisposition::Included {
                    included.push(item.id.clone());
                } else {
                    expected_ready = false;
                }
            } else {
                excluded += 1;
                match item.disposition {
                    EnergyAttachmentDisposition::Included => {
                        return Err(invalid(
                            "report.attachments",
                            "Unchecked attachment cannot be included",
                        ))
                    }
                    EnergyAttachmentDisposition::Missing | EnergyAttachmentDisposition::Invalid => {
                        missing += 1
                    }
                    EnergyAttachmentDisposition::Inapplicable => inapplicable += 1,
                    EnergyAttachmentDisposition::Excluded => {}
                }
            }
        }
        if self.ready != expected_ready || !expected_ready {
            return Err(invalid("report.attachments","Resolve checked missing, invalid or model-inapplicable attachments explicitly before export"));
        }
        included.sort();
        Ok(EnergyReportAttachmentSummary {
            included_ids: included,
            excluded_count: excluded,
            missing_unselected_count: missing,
            inapplicable_unselected_count: inapplicable,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportAttachmentSummary {
    pub included_ids: Vec<String>,
    pub excluded_count: usize,
    pub missing_unselected_count: usize,
    pub inapplicable_unselected_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportWorkbenchData {
    pub schema_version: String,
    pub base: EnergyReportData,
    pub base_dataset_sha256: String,
    pub metadata: EnergyReportMetadata,
    pub profiles: Vec<EnergyReportProfile>,
    pub typed_comparison: ThermalInputDifferences,
    pub attachments: EnergyReportAttachmentSummary,
    pub template_sha256: String,
    pub derived_sha256: String,
}
impl EnergyReportWorkbenchData {
    pub fn new(
        base: EnergyReportData,
        metadata: EnergyReportMetadata,
        profiles: Vec<EnergyReportProfile>,
        attachments: EnergyReportAttachmentSummary,
        template_sha256: String,
    ) -> Result<Self> {
        let typed_comparison = compare_thermal_inputs_v1(
            &base.baseline.model,
            &base.baseline.study,
            &base.proposed.model,
            &base.proposed.study,
        )?;
        let mut value = Self {
            schema_version: ENERGY_WORKBENCH_REPORT_SCHEMA.into(),
            base_dataset_sha256: base.dataset_sha256()?,
            base,
            metadata,
            profiles,
            typed_comparison,
            attachments,
            template_sha256,
            derived_sha256: String::new(),
        };
        value.derived_sha256 = value.identity()?;
        value.validate()?;
        Ok(value)
    }
    fn identity(&self) -> Result<String> {
        canonical_hash(&(
            ENERGY_WORKBENCH_REPORT_SCHEMA,
            &self.base_dataset_sha256,
            &self.metadata,
            &self.profiles,
            &self.typed_comparison,
            &self.attachments,
            &self.template_sha256,
        ))
    }
    pub fn validate(&self) -> Result<()> {
        self.base.validate()?;
        self.metadata.validate()?;
        hash("report.template", &self.template_sha256)?;
        hash("report.derived", &self.derived_sha256)?;
        if self.schema_version != ENERGY_WORKBENCH_REPORT_SCHEMA
            || self.base.dataset_sha256()? != self.base_dataset_sha256
            || self.identity()? != self.derived_sha256
        {
            return Err(invalid(
                "report_workbench.identity",
                "Derived report identity or underlying immutable dataset differs",
            ));
        }
        let expected = compare_thermal_inputs_v1(
            &self.base.baseline.model,
            &self.base.baseline.study,
            &self.base.proposed.model,
            &self.base.proposed.study,
        )?;
        if self.typed_comparison != expected {
            return Err(invalid(
                "report_workbench.comparison",
                "Typed comparison must be recomputed from the retained snapshots",
            ));
        }
        if self.profiles.is_empty()
            || self.profiles.len() > 2
            || self.profiles.iter().copied().collect::<BTreeSet<_>>().len() != self.profiles.len()
        {
            return Err(invalid(
                "report_workbench.profiles",
                "Select executive and/or technical exactly once",
            ));
        }
        let mut ids = self
            .base
            .evidence
            .iter()
            .filter(|e| e.included)
            .map(|e| e.id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        let excluded = self.attachments.excluded_count;
        if self.base.evidence.iter().any(|e| !e.included)
            || ids != self.attachments.included_ids
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || ids.len() > 1024
            || excluded > 1024
            || ids.len() + excluded > 1024
            || self.attachments.missing_unselected_count > excluded
            || self.attachments.inapplicable_unselected_count > excluded
            || self.attachments.missing_unselected_count
                + self.attachments.inapplicable_unselected_count
                > excluded
        {
            return Err(invalid("report_workbench.attachments","Public dataset must include only explicitly selected applicable evidence, with no excluded private metadata"));
        }
        Ok(())
    }
    /// Only the new typed policy informs new human-readable service claims.
    pub fn service_disclosures(&self) -> Vec<String> {
        let mut result = self.typed_comparison.limitations.clone();
        result.push(if self.typed_comparison.unchanged_service_inputs {"No service-input change was identified by the versioned typed comparison. This does not establish measured service equivalence."} else {"Service inputs differ. Energy savings must be interpreted together with the listed occupancy, ventilation, schedule, flow and setpoint changes."}.into());
        result.push(self.base.comparison.lighting_service_equivalence.clone());
        result.push("Cooling only: heating and lower-temperature service hours are not evaluated; no lower service threshold is defined.".into());
        for c in [&self.base.baseline, &self.base.proposed] {
            result.push(format!("{} weather admission profile: {}. Admission is distinct from scientific qualification and climatic applicability.",c.label,c.weather_summary.binding.profile_id));
            if c.weather_summary.binding.profile_id == weather::ENERGY_DRY_ENVELOPE_PROFILE {
                result.push("Restricted dry-envelope method: original missing albedo/precipitation sentinels are preserved. Rain/snow surface effects and water, green-roof, ground, PV and EMS consumers are outside this method. Monthly exterior ground reflectance is explicitly 0.2, not EPW albedo. The adapter enforces the object/control restriction; this is not general weather suitability.".into());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> EnergyReportWorkbenchData {
        let mut base = synthetic_energy_report().unwrap();
        base.evidence.retain(|e| e.included);
        let mut ids = base
            .evidence
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        EnergyReportWorkbenchData::new(
            base,
            EnergyReportMetadata::default(),
            vec![
                EnergyReportProfile::Executive,
                EnergyReportProfile::Technical,
            ],
            EnergyReportAttachmentSummary {
                included_ids: ids,
                ..Default::default()
            },
            "a".repeat(64),
        )
        .unwrap()
    }
    #[test]
    fn metadata_changes_only_the_derived_identity_and_tampering_rejects() {
        let a = report();
        let mut m = a.metadata.clone();
        m.title = "Original issue <script> — 能源".into();
        m.issue_date = Some("2024-02-29".into());
        let b = EnergyReportWorkbenchData::new(
            a.base.clone(),
            m,
            a.profiles.clone(),
            a.attachments.clone(),
            a.template_sha256.clone(),
        )
        .unwrap();
        assert_eq!(a.base, b.base);
        assert_eq!(a.base_dataset_sha256, b.base_dataset_sha256);
        assert_ne!(a.derived_sha256, b.derived_sha256);
        let mut bad = b;
        bad.typed_comparison.unchanged_service_inputs =
            !bad.typed_comparison.unchanged_service_inputs;
        assert!(bad.validate().is_err());
        for invalid in [
            "2023-02-29",
            "2024-13-01",
            "2024-00-01",
            "0000-01-01",
            "2024-1-01",
        ] {
            assert!(date(invalid).is_err());
        }
    }
    #[test]
    fn selected_attachment_never_disappears_and_unselected_metadata_is_not_public() {
        for disposition in [
            EnergyAttachmentDisposition::Missing,
            EnergyAttachmentDisposition::Inapplicable,
            EnergyAttachmentDisposition::Invalid,
            EnergyAttachmentDisposition::Excluded,
        ] {
            let p = EnergyAttachmentPreflight {
                schema_version: "greenlens.energy-attachment-preflight.v1".into(),
                ready: false,
                items: vec![EnergyAttachmentDecision {
                    id: "private-attachment".into(),
                    selected: true,
                    disposition: disposition.clone(),
                    reason: "Review selection".into(),
                }],
            };
            assert!(p.summary().is_err());
            let mut p = p;
            p.ready = true;
            p.items[0].selected = false;
            let s = p.summary().unwrap();
            let json = serde_json::to_string(&s).unwrap();
            assert!(!json.contains("private-attachment"));
            assert_eq!(s.excluded_count, 1);
        }
    }
    #[test]
    fn new_fan_interpretation_preserves_old_dataset_without_reusing_old_warning() {
        let d = report();
        for row in d
            .typed_comparison
            .differences
            .iter()
            .filter(|r| r.object.kind == ThermalObjectKind::SupplyFan)
        {
            assert_eq!(row.service_effect, ThermalServiceEffect::None);
        }
        assert!(!d
            .service_disclosures()
            .iter()
            .any(|s| s.contains("fan efficiency changes ventilation service")));
        assert!(d.validate().is_ok());
        let mut malformed = d.clone();
        malformed.attachments.missing_unselected_count = usize::MAX;
        assert!(malformed.validate().is_err());
    }
}
