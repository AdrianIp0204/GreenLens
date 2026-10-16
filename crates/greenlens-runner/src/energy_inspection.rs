//! Bounded read-only application queries shared by native callers. No command
//! evaluation, imported recipes, simulation launch or project mutation.
use crate::{energy_services as service, Project};
use anyhow::Result;
use greenlens_core::energy::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Deserialize)]
#[serde(tag = "command", content = "arguments")]
pub enum EnergyEvidenceQuery {
    #[serde(rename = "energy_default_runs")]
    Defaults(ProjectQuery),
    #[serde(rename = "energy_run_catalog")]
    Catalog(CatalogQuery),
    #[serde(rename = "energy_result")]
    Result(RunQuery),
    #[serde(rename = "energy_rows")]
    Rows(RowsQuery),
    #[serde(rename = "energy_navigate")]
    Navigate(NavigateQuery),
    #[serde(rename = "energy_navigate_pair")]
    NavigatePair(NavigatePairQuery),
    #[serde(rename = "energy_compare")]
    Compare(PairQuery),
    #[serde(rename = "energy_report_preflight")]
    Preflight(PairQuery),
    #[serde(rename = "energy_sensitivity_inspect")]
    Sensitivity(SessionQuery),
    #[serde(rename = "energy_sensitivity_export_selection")]
    ExportSelection(PairQuery),
    #[serde(rename = "report_issue_inspect")]
    Issue(SessionQuery),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectQuery {
    pub path: String,
    pub project: Project,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogQuery {
    pub path: String,
    pub project: Project,
    pub request: crate::energy_catalog::EnergyRunCatalogRequest,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunQuery {
    pub path: String,
    pub project: Project,
    pub run_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowsQuery {
    pub path: String,
    pub project: Project,
    pub run_id: String,
    pub start: usize,
    pub count: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigateQuery {
    pub path: String,
    pub project: Project,
    pub run_id: String,
    pub request: EnergyNavigationRequest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairQuery {
    pub path: String,
    pub project: Project,
    pub a: String,
    pub b: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigatePairQuery {
    pub path: String,
    pub project: Project,
    pub a: String,
    pub b: String,
    pub request: EnergyNavigationRequest,
    pub anchor: crate::energy_navigation::EnergyNavigationAnchor,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionQuery {
    pub path: String,
    pub project: Project,
    pub id: String,
}

impl EnergyEvidenceQuery {
    fn path(&self) -> &str {
        match self {
            Self::Defaults(q) => &q.path,
            Self::Catalog(q) => &q.path,
            Self::Result(q) => &q.path,
            Self::Rows(q) => &q.path,
            Self::Navigate(q) => &q.path,
            Self::NavigatePair(q) => &q.path,
            Self::Compare(q) | Self::Preflight(q) | Self::ExportSelection(q) => &q.path,
            Self::Sensitivity(q) | Self::Issue(q) => &q.path,
        }
    }
    pub fn execute(
        self,
        cancel: Arc<AtomicBool>,
        progress: impl Fn(crate::project_open::OpenProgress) + Send + Sync + 'static,
    ) -> Result<Value> {
        let path = self.path().to_owned();
        let root = Path::new(&path);
        crate::project_open::inspect_evidence(root, cancel.clone(), progress, || match self {
            Self::Defaults(q) => Ok(serde_json::to_value(service::default_run_ids(
                root, &q.project,
            )?)?),
            Self::Catalog(q) => Ok(serde_json::to_value(crate::energy_catalog::catalog(
                root, &q.project, &q.request,
            )?)?),
            Self::Result(q) => {
                let case = service::report_case(root, &q.project, &q.run_id)?;
                let presentation = case.presentation()?;
                Ok(json!({"case":case,"presentation":presentation}))
            }
            Self::Rows(q) => service::rows(root, &q.project, &q.run_id, q.start, q.count),
            Self::Navigate(q) => Ok(serde_json::to_value(crate::energy_navigation::query(
                root, &q.project, &q.run_id, &q.request,
            )?)?),
            Self::NavigatePair(q) => Ok(serde_json::to_value(crate::energy_navigation::pair(
                root, &q.project, &q.a, &q.b, &q.request, q.anchor,
            )?)?),
            Self::Compare(q) => {
                let data = service::report_data(root, &q.project, &q.a, &q.b)?;
                let presentation = data.presentation()?;
                let typed_comparison = compare_thermal_inputs_v1(
                    &data.baseline.model,
                    &data.baseline.study,
                    &data.proposed.model,
                    &data.proposed.study,
                )?;
                Ok(
                    json!({"data":data,"presentation":presentation,"typed_comparison":typed_comparison}),
                )
            }
            Self::Preflight(q) => Ok(serde_json::to_value(service::report_attachment_preflight(
                root, &q.project, &q.a, &q.b,
            )?)?),
            Self::Sensitivity(q) => Ok(serde_json::to_value(
                crate::energy_sensitivity_workflow::inspect(root, &q.project, &q.id)?,
            )?),
            Self::ExportSelection(q) => Ok(serde_json::to_value(
                crate::energy_sensitivity_workflow::export_selection(root, &q.project, &q.a, &q.b)?,
            )?),
            Self::Issue(q) => Ok(serde_json::to_value(
                crate::report_issues::inspect_in_operation(root, &q.project, &q.id, &cancel)?,
            )?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_surface_rejects_execution_and_unexpected_arguments() {
        assert!(serde_json::from_value::<EnergyEvidenceQuery>(
            json!({"command":"energy_start","arguments":{}})
        )
        .is_err());
        let mut query = json!({"command":"energy_default_runs","arguments":{"path":"unused","project":crate::example()}});
        assert!(serde_json::from_value::<EnergyEvidenceQuery>(query.clone()).is_ok());
        query["arguments"]["shell"] = json!("unavailable");
        assert!(serde_json::from_value::<EnergyEvidenceQuery>(query).is_err());
    }
}
