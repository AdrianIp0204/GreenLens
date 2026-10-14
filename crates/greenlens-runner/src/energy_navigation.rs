//! Engine-free bounded navigation. Uses the pinned retained contract, existing
//! series reader and decoder; never changes a run or its historical report.
use crate::{
    energy::{read_result_metadata, EnergyRunRecord},
    energy_series::EnergySeriesReader,
    Project,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use serde::{Deserialize, Serialize};
use std::path::Path;

const INTEGRITY_SCOPE: &str = "Pinned run metadata and compiled recipe checked; every requested or searched series chunk is checksum-checked and decoded. This query does not attest to unrelated retained files or replace full open/export integrity checks.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyNavigationResult {
    pub schema_version: String,
    pub run_id: String,
    pub record_sha256: String,
    pub physical_input_sha256: String,
    pub method: EnergyMethodIdentity,
    pub retained_facility_peak: CoincidentDemand,
    pub data: Option<EnergyInvestigationData>,
    /// Common intervals searched before the first match, or through year end.
    pub searched_interval_count: usize,
    /// All decoded source rows, including bounded lookahead within a chunk.
    pub decoded_source_interval_count: usize,
    pub integrity_scope: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyNavigationAnchor {
    Baseline,
    Proposed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyPairNavigationResult {
    pub schema_version: String,
    pub anchor: EnergyNavigationAnchor,
    pub baseline: EnergyNavigationResult,
    pub proposed: EnergyNavigationResult,
    pub shared_demand_scale: Option<EnergyDemandScale>,
    pub input_comparison: EnergyInputComparison,
    pub limitations: Vec<String>,
}

struct RetainedQuery {
    run: EnergyRunRecord,
    reader: EnergySeriesReader,
    decoder: greenlens_energyplus::Decoder,
    conditioned_zones: Vec<String>,
}
impl RetainedQuery {
    fn open(root: &Path, project: &Project, id: &str) -> Result<Self> {
        let summary = project
            .energy
            .as_ref()
            .and_then(|e| e.runs.iter().find(|r| r.id == id))
            .context("Selected energy snapshot is not retained in this project")?;
        let run = read_result_metadata(root, summary)?;
        let reader = EnergySeriesReader::open(&root.join(&summary.path).join("series"))?;
        ensure!(
            reader.manifest() == &run.series,
            "Series manifest differs from the pinned record"
        );
        let decoder = greenlens_energyplus::Decoder::new(&run.model, &run.study)?;
        let conditioned_zones = run
            .model
            .zones
            .iter()
            .filter(|z| z.cooling_system.is_some())
            .map(|z| z.id.clone())
            .collect();
        Ok(Self {
            run,
            reader,
            decoder,
            conditioned_zones,
        })
    }
    fn window(&self, window: EnergyQueryWindow) -> Result<EnergyInvestigationData> {
        let (start, count) = window.native_range(self.run.series.interval_minutes)?;
        let rows = self
            .reader
            .rows(start, count)?
            .into_iter()
            .enumerate()
            .map(|(offset, values)| self.decoder.decode(start + offset, &values))
            .collect::<Result<Vec<_>>>()?;
        Ok(investigation_data(
            window,
            self.run.series.interval_minutes,
            &rows,
            &self.conditioned_zones,
        )?)
    }
    fn result(
        &self,
        data: Option<EnergyInvestigationData>,
        searched: usize,
        decoded: usize,
    ) -> Result<EnergyNavigationResult> {
        Ok(EnergyNavigationResult {
            schema_version: ENERGY_INVESTIGATION_SCHEMA.into(),
            run_id: self.run.id.clone(),
            record_sha256: canonical_hash(&self.run)?,
            physical_input_sha256: self.run.provenance.physical_input_hash.clone(),
            method: self.run.method.clone(),
            retained_facility_peak: self.run.totals.coincident_peak.clone(),
            data,
            searched_interval_count: searched,
            decoded_source_interval_count: decoded,
            integrity_scope: INTEGRITY_SCOPE.into(),
        })
    }
    fn query(&self, request: &EnergyNavigationRequest) -> Result<EnergyNavigationResult> {
        let window = investigation_window(request, &self.run.totals.coincident_peak.timestamp)?;
        if let EnergyNavigationRequest::OccupiedUnmet { zone_id, .. } = request {
            if let Some(id) = zone_id {
                ensure!(
                    self.conditioned_zones.contains(id),
                    "Unmet search requires an existing conditioned zone ID"
                );
            }
            let (data, searched, decoded) =
                search_unmet(window.start_interval, zone_id.as_deref(), |w| {
                    self.window(w)
                })?;
            self.result(data, searched, decoded)
        } else {
            let data = self.window(window)?;
            let decoded = data.comfort.len();
            self.result(Some(data), 0, decoded)
        }
    }
}

/// Search the finite remaining year with constant response/memory bounds. No
/// favorable scan selection: always the first occupied unmet source interval.
fn search_unmet(
    start: usize,
    zone: Option<&str>,
    mut read: impl FnMut(EnergyQueryWindow) -> Result<EnergyInvestigationData>,
) -> Result<(Option<EnergyInvestigationData>, usize, usize)> {
    ensure!(
        start < ENERGY_INTERVALS,
        "Unmet search starts outside the retained year"
    );
    let mut cursor = start;
    let mut decoded = 0;
    while cursor < ENERGY_INTERVALS {
        let window = EnergyQueryWindow {
            start_interval: cursor,
            count: MAX_INVESTIGATION_INTERVALS.min(ENERGY_INTERVALS - cursor),
            focus_interval: None,
        };
        let data = read(window)?;
        ensure!(
            data.window == window,
            "Search reader returned a different requested window"
        );
        decoded += data.comfort.len();
        if let Some(index) = first_occupied_unmet(&data, zone) {
            ensure!(
                (cursor..cursor + window.count).contains(&index),
                "Unmet match lies outside searched range"
            );
            let point = data
                .demand_10min
                .iter()
                .find(|p| p.timestamp.interval_index == index)
                .context("Unmet interval has no demand point")?
                .clone();
            let comfort = data
                .comfort
                .into_iter()
                .filter(|p| p.timestamp.common_interval_index == index)
                .collect();
            let selected = EnergyInvestigationData {
                window: EnergyQueryWindow {
                    start_interval: index,
                    count: 1,
                    focus_interval: Some(index),
                },
                source_interval_minutes: data.source_interval_minutes,
                demand_10min: vec![point],
                comfort,
            };
            return Ok((Some(selected), index - start + 1, decoded));
        }
        cursor += window.count;
    }
    Ok((None, ENERGY_INTERVALS - start, decoded))
}

pub fn query(
    root: &Path,
    project: &Project,
    run_id: &str,
    request: &EnergyNavigationRequest,
) -> Result<EnergyNavigationResult> {
    RetainedQuery::open(root, project, run_id)?.query(request)
}

type PairInput<'a> = (
    &'a ThermalModel,
    &'a EnergyStudy,
    &'a EnergyMethodIdentity,
    &'a MeterLedger,
);
fn pair_compatibility(
    (a, sa, ma, la): PairInput<'_>,
    (b, sb, mb, lb): PairInput<'_>,
) -> Result<EnergyInputComparison> {
    ensure!(ma == mb && la == lb, "A/B navigation requires the same complete method, engine, numerical profile and meter ledger");
    let comparison = compare_energy_inputs(a, sa, b, sb)?;
    ensure!(comparison.comparable_geometry_weather_calendar, "A/B navigation requires matching geometry, site, weather, calendar and source timestep; mixed-timestep comparisons belong to separately declared sensitivity evidence");
    Ok(comparison)
}

/// Both returned series have exactly the anchor's timestamp range. The
/// secondary case never silently jumps to its own peak or unmet interval.
pub fn pair(
    root: &Path,
    project: &Project,
    a_id: &str,
    b_id: &str,
    request: &EnergyNavigationRequest,
    anchor: EnergyNavigationAnchor,
) -> Result<EnergyPairNavigationResult> {
    let a = RetainedQuery::open(root, project, a_id)?;
    let b = RetainedQuery::open(root, project, b_id)?;
    ensure!(
        a.run.alternative_id == "A" && b.run.alternative_id == "B",
        "Select explicit baseline A and proposed B snapshots"
    );
    let comparison = pair_compatibility(
        (&a.run.model, &a.run.study, &a.run.method, &a.run.ledger),
        (&b.run.model, &b.run.study, &b.run.method, &b.run.ledger),
    )?;
    let (first, other) = match anchor {
        EnergyNavigationAnchor::Baseline => (&a, &b),
        EnergyNavigationAnchor::Proposed => (&b, &a),
    };
    let selected = first.query(request)?;
    let secondary = if let Some(data) = &selected.data {
        let other_data = other.window(data.window)?;
        let decoded = other_data.comfort.len();
        other.result(Some(other_data), 0, decoded)?
    } else {
        other.result(None, 0, 0)?
    };
    let (baseline, proposed) = match anchor {
        EnergyNavigationAnchor::Baseline => (selected, secondary),
        EnergyNavigationAnchor::Proposed => (secondary, selected),
    };
    let shared_demand_scale = match (&baseline.data, &proposed.data) {
        (Some(a), Some(b)) => Some(shared_demand_scale(&a.demand_10min, &b.demand_10min)?),
        (None, None) => None,
        _ => anyhow::bail!("A/B investigation range mismatch"),
    };
    Ok(EnergyPairNavigationResult { schema_version: "greenlens.energy-pair-investigation.v1".into(), anchor,
        baseline, proposed, shared_demand_scale, input_comparison: comparison,
        limitations: vec!["Demand is summed to complete common ten-minute intervals; occupied cooling-unmet conditions retain native five-/ten-minute resolution and the unchanged 0.2 C threshold.".into(),
            "An unmet search returns the first match for the explicitly selected anchor. No match says nothing about the other case; select that case as anchor to search it.".into(),
            "Navigation does not assert service equivalence, numerical sensitivity, scientific qualification or compliance.".into()] })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_year_unmet_search_is_bounded_and_returns_first_exact_subinterval() {
        let mut calls = 0;
        let (found, searched, decoded) = search_unmet(0, Some("z"), |window| {
            calls += 1;
            assert!(window.count <= 1008);
            let (start, count) = window.native_range(5).unwrap();
            let rows = (start..start + count)
                .map(|i| EnergyInterval {
                    index: i,
                    facility_j: 300_000.,
                    end_uses_j: [(EnergyEndUse::CoolingSystem, 300_000.)].into(),
                    comfort: vec![ComfortSample {
                        zone_id: "z".into(),
                        occupied: true,
                        air_temperature_c: if i == 104_999 { 25. } else { 24. },
                        cooling_setpoint_c: 24.,
                    }],
                })
                .collect::<Vec<_>>();
            Ok(investigation_data(window, 5, &rows, &["z".into()])?)
        })
        .unwrap();
        let data = found.unwrap();
        assert_eq!(calls, 53);
        assert_eq!(searched, 52_500);
        assert_eq!(decoded, 105_120);
        assert_eq!(data.window.count, 1);
        assert_eq!(data.demand_10min.len(), 1);
        assert_eq!(data.comfort.len(), 2);
        assert_eq!(data.comfort[1].timestamp.source_interval_index, 104_999);
        assert_eq!(data.comfort[1].occupied_unmet_zone_ids, vec!["z"]);
        let (found, searched, decoded) = search_unmet(52559, None, |window| {
            Ok(investigation_data(
                window,
                10,
                &[EnergyInterval {
                    index: 52559,
                    facility_j: 0.,
                    end_uses_j: [].into(),
                    comfort: vec![],
                }],
                &[],
            )?)
        })
        .unwrap();
        assert!(found.is_none());
        assert_eq!((searched, decoded), (1, 1));
    }
    #[test]
    fn pair_preserves_complete_historical_compatibility() {
        let r = synthetic_energy_report().unwrap();
        let a = &r.baseline;
        let b = &r.proposed;
        let compare = |study: &EnergyStudy, method: &EnergyMethodIdentity, model: &ThermalModel| {
            pair_compatibility(
                (&a.model, &a.study, &a.method, &a.meter_ledger),
                (model, study, method, &b.meter_ledger),
            )
        };
        assert!(compare(&b.study, &b.method, &b.model).is_ok());
        let mut study = b.study.clone();
        study.zone_timesteps_per_hour = 12;
        assert!(compare(&study, &b.method, &b.model).is_err());
        let mut study = b.study.clone();
        study.calendar.utc_offset_minutes += 60;
        assert!(compare(&study, &b.method, &b.model).is_err());
        let mut method = b.method.clone();
        method.recipe_sha256 = "f".repeat(64);
        assert!(compare(&b.study, &method, &b.model).is_err());
        let mut model = b.model.clone();
        for z in &mut model.zones {
            z.origin_m[0] += 1.;
        }
        assert!(compare(&b.study, &b.method, &model).is_err());
        let serialized = serde_json::to_vec(&r).unwrap();
        compare(&b.study, &b.method, &b.model).unwrap();
        r.validate().unwrap();
        assert_eq!(serde_json::to_vec(&r).unwrap(), serialized);
    }
}
