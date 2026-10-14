//! Input-only support checks. No engine discovery, process, file write or new
//! simulation policy. Historical retained-contract validation stays unchanged.
use greenlens_core::energy::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AdapterPreflight {
    pub schema_version: String,
    pub supported: bool,
    pub method_id: String,
    pub issues: Vec<ThermalIssue>,
    pub generated_epjson_bytes: Option<u64>,
    pub coverage: String,
}

pub fn preflight_model(model: &ThermalModel, study: &EnergyStudy) -> AdapterPreflight {
    let core = preflight_thermal_inputs(model, &study.calendar);
    let mut issues = core.issues;
    if core.valid {
        if let Err(e) = study.validate(model) {
            issues.push(model_level_issue(
                "energy_study.invalid",
                ThermalIssueKind::InvalidInput,
                e.to_string(),
                "Correct the weather binding, calendar or supported timestep before running.",
            ));
        }
        for z in &model.zones {
            if let Some(d) = &z.cooling_system {
                for (path, actual, expected) in [
                    ("/rated_indoor_wet_bulb_c", d.rated_indoor_wet_bulb_c, 19.44),
                    ("/rated_outdoor_dry_bulb_c", d.rated_outdoor_dry_bulb_c, 35.),
                ] {
                    // Exact existing model::hvac predicate, not a rounded display
                    // comparison. render_model remains the authoritative check.
                    if (actual - expected).abs() >= 1e-9 {
                        issues.push(ThermalIssue {code:"energyplus.dx_rating_unsupported".into(),kind:ThermalIssueKind::UnsupportedAdapter,severity:ThermalIssueSeverity::Error,
                            object:ThermalObjectRef::new(ThermalObjectKind::CoolingSystem,&d.id,Some(&z.id)),related_objects:vec![],field_path:path.into(),
                            message:format!("The pinned single-speed DX adapter requires {expected} C at this rating field; the input is {actual} C."),
                            remedy:"Supply performance data at the supported 19.44 C indoor wet-bulb / 35 C outdoor dry-bulb rating. No automatic rating conversion is performed.".into()});
                    }
                }
            }
        }
    }
    let generated_epjson_bytes = if issues.is_empty() {
        match super::render_model(model, study) {
            Ok(bytes) => Some(bytes.len() as u64),
            Err(e) => {
                issues.push(model_level_issue("energyplus.render_unsupported",ThermalIssueKind::UnsupportedAdapter,e.to_string(),"Review the bounded expanded schedules and generated model. Simplify only by an explicit accepted input edit; no fields are silently dropped."));
                None
            }
        }
    } else {
        None
    };
    let method_id = super::method_identity(study)
        .map(|method| method.method_id)
        .unwrap_or_else(|_| "unavailable".into());
    AdapterPreflight {schema_version:"greenlens.energyplus-preflight.v1".into(),supported:issues.is_empty(),method_id,issues,generated_epjson_bytes,
        coverage:"Core first-failure validation, study validation, every supported DX rating, and deterministic owned input generation including existing schedule-field/64 MiB bounds. No engine installation, EPW file integrity, operating curve-domain coverage, output success or scientific qualification is asserted.".into()}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ThermalModel, EnergyStudy) {
        let report = synthetic_energy_report().unwrap();
        (report.baseline.model, report.baseline.study)
    }
    #[test]
    fn preflight_rejects_each_rating_before_execution_and_preserves_render_bytes() {
        let (m, s) = fixture();
        let before = super::super::render_model(&m, &s).unwrap();
        let p = preflight_model(&m, &s);
        assert!(p.supported);
        assert_eq!(p.generated_epjson_bytes, Some(before.len() as u64));
        assert_eq!(before, super::super::render_model(&m, &s).unwrap());
        let mut bad = m.clone();
        let h = bad.zones[1].cooling_system.as_mut().unwrap();
        h.rated_indoor_wet_bulb_c = 20.;
        h.rated_outdoor_dry_bulb_c = 30.;
        assert!(s.validate(&bad).is_ok());
        let p = preflight_model(&bad, &s);
        assert!(!p.supported);
        assert_eq!(p.issues.len(), 2);
        assert!(p
            .issues
            .iter()
            .all(|i| i.object.zone_id.as_deref() == Some(bad.zones[1].id.as_str())));
        assert_eq!(p.issues[0].field_path, "/rated_indoor_wet_bulb_c");
        assert_eq!(p.issues[1].field_path, "/rated_outdoor_dry_bulb_c");
        assert!(super::super::render_model(&bad, &s).is_err());
    }
    #[test]
    fn preflight_and_existing_strict_admission_agree_on_failure_paths() {
        let (mut m, mut s) = fixture();
        s.zone_timesteps_per_hour = 4;
        assert!(!preflight_model(&m, &s).supported);
        assert!(super::super::render_model(&m, &s).is_err());
        s.zone_timesteps_per_hour = 6;
        m.zones[0].cooling_system.as_mut().unwrap().gross_rated_cop = f64::NAN;
        let p = preflight_model(&m, &s);
        assert!(!p.supported);
        assert_eq!(p.issues[0].field_path, "/gross_rated_cop");
        assert!(p.generated_epjson_bytes.is_none());
    }

    #[test]
    fn preflight_reports_the_selected_recipe_method_without_relabeling_history() {
        let (m, strict) = fixture();
        let historical_recipe = super::super::recipe_bytes().to_vec();
        let historical_method = super::super::method_identity(&strict).unwrap();
        let mut dry = strict.clone();
        dry.weather = weather::parse_energy_epw_with_profile(
            &weather::synthetic_energy_epw(),
            weather::EnergyWeatherProfile::DryEnvelopeV1,
        )
        .unwrap()
        .data
        .binding();
        let dry_method = super::super::method_identity(&dry).unwrap();
        let preflight = preflight_model(&m, &dry);
        assert!(preflight.supported);
        assert_eq!(preflight.method_id, dry_method.method_id);
        assert_ne!(preflight.method_id, historical_method.method_id);
        assert_ne!(dry_method.recipe_sha256, historical_method.recipe_sha256);
        assert_eq!(
            preflight_model(&m, &strict).method_id,
            historical_method.method_id
        );
        assert_eq!(
            super::super::recipe_for_study(&strict).unwrap(),
            historical_recipe
        );
        assert_eq!(
            super::super::method_identity(&strict).unwrap(),
            historical_method
        );
        dry.weather.profile_id = "unreviewed-profile".into();
        let unsupported = preflight_model(&m, &dry);
        assert!(!unsupported.supported);
        assert_eq!(unsupported.method_id, "unavailable");
        assert!(unsupported.generated_epjson_bytes.is_none());
    }
}
