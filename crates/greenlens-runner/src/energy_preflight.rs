//! Workbench readiness is distinct from evidence integrity and qualification.
use crate::{energy_workbench, Project};
use anyhow::{Context, Result};
use greenlens_core::energy::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyPreflight {
    pub schema_version: String,
    pub alternative_id: String,
    pub can_prepare: bool,
    pub issues: Vec<ThermalIssue>,
    pub coverage: String,
}
pub fn inputs(project: &Project, alternative: &str) -> Result<EnergyPreflight> {
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let design = energy.designs.get(alternative).context("Choose A or B")?;
    let mut issues = if let Some(study) = &design.study {
        greenlens_energyplus::preflight_model(&design.model, study).issues
    } else {
        let mut issues = preflight_thermal_inputs(
            &design.model,
            &energy_workbench::calendar_for(project, alternative)?,
        )
        .issues;
        issues.push(model_level_issue("weather.not_assigned",ThermalIssueKind::MissingEvidence,"No retained thermal weather and study are assigned.","Import suitable EPW weather and explicitly configure its non-leap calendar before calculating."));
        issues
    };
    if let Some(study) = &design.study {
        if !energy.weather.iter().any(|w| w.binding == study.weather) {
            issues.push(model_level_issue("weather.not_retained",ThermalIssueKind::MissingEvidence,"Study weather is not retained in this project.","Import and retain the exact weather asset. An original external file path is insufficient."));
        }
    }
    let mut science = model_level_issue("science.separate_assessment",ThermalIssueKind::UnassessedScience,"Input readiness is not scientific qualification. User models do not inherit the synthetic example's limited registry status.","Inspect the completed exact-method assessment and run an explicit numerical sensitivity check where appropriate.");
    science.severity = ThermalIssueSeverity::Information;
    issues.push(science);
    Ok(EnergyPreflight { schema_version:"greenlens.energy-preflight.v1".into(),alternative_id:alternative.into(),can_prepare:!issues.iter().any(|i|i.severity==ThermalIssueSeverity::Error),issues,coverage:"Authoritative core/study validation, owned adapter restrictions and retained weather-reference presence. Local dependency hashes, weather bytes and successful execution are checked separately before native work; no physical accuracy is asserted.".into() })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyReadiness {
    pub schema_version: String,
    pub energyplus_ready: bool,
    pub typst_and_fonts_ready: bool,
    pub issues: Vec<ThermalIssue>,
}
pub fn dependencies(engine_dir: Option<&Path>, typst_dir: Option<&Path>) -> DependencyReadiness {
    let engine = crate::energy::doctor(engine_dir);
    let typst = crate::energy_report::TypstConfig::discover(typst_dir);
    let mut issues = Vec::new();
    if engine.is_err() {
        issues.push(model_level_issue("dependency.energyplus",ThermalIssueKind::MissingDependency,"EnergyPlus is missing or fails its pinned executable/schema/runtime inventory.","Install the pinned package with scripts/install-energyplus.ps1 or select the verified native installation. Calculation remains offline after setup."));
    }
    if typst.is_err() {
        issues.push(model_level_issue("dependency.typst",ThermalIssueKind::MissingDependency,"Typst or the pinned local fonts are missing or altered.","Install the pinned renderer and fonts with scripts/install-typst.ps1 or select their verified installation. HTML inspection is available without PDF rendering."));
    }
    DependencyReadiness {
        schema_version: "greenlens.energy-dependencies.v1".into(),
        energyplus_ready: engine.is_ok(),
        typst_and_fonts_ready: typst.is_ok(),
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inputs_explain_missing_weather_and_adapter_rating_before_job() {
        let mut p = crate::energy_services::example_document();
        let check = inputs(&p, "A").unwrap();
        assert!(!check.can_prepare);
        assert!(check
            .issues
            .iter()
            .any(|i| i.code == "weather.not_assigned"));
        let fixture = synthetic_energy_report().unwrap();
        let d = p.energy.as_mut().unwrap().designs.get_mut("A").unwrap();
        d.study = Some(fixture.baseline.study);
        d.model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .rated_outdoor_dry_bulb_c = 34.;
        let check = inputs(&p, "A").unwrap();
        assert!(!check.can_prepare);
        assert!(check.issues.iter().any(
            |i| i.code == "energyplus.dx_rating_unsupported" && i.object.id == "zone1_cooling"
        ));
        assert!(check
            .issues
            .iter()
            .any(|i| i.kind == ThermalIssueKind::UnassessedScience
                && i.severity == ThermalIssueSeverity::Information));
    }
}
