//! Additive architectural scope and provenance. This never changes a thermal
//! model, a physical simulation identity, or scientific qualification.
//!
//! Callers supply admitted reference metadata and models produced by
//! `validate_thermal_model`. Geometry chunk integrity remains the reference
//! service's responsibility; a scope assessment does not claim to verify it.
use crate::energy::{
    hash, invalid, text, thermal_object_snapshots, ThermalBoundary, ValidatedThermalModel,
};
use crate::reference::{validate_manifest, ReferenceModel};
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ARCHITECTURAL_SCOPE_SCHEMA: &str = "greenlens.architectural-scope.v1";
pub const SCOPE_ASSESSMENT_SCHEMA: &str = "greenlens.architectural-scope-assessment.v1";
pub const MAX_SCOPE_MAPPINGS: usize = 20_000;
pub const MAX_SCOPE_ASSUMPTIONS: usize = 2048;
pub const MAX_SCOPE_RECONCILIATIONS: usize = 4096;
pub const MAX_SCOPE_BYTES: usize = 8 * 1024 * 1024;
pub const REFERENCE_REPORT_VIEW_SCHEMA: &str = "greenlens.reference-report-view.v1";
pub const MAX_REFERENCE_REPORT_VIEWS: usize = 8;
/// Bookkeeping tolerance, not a geometry approximation or qualification limit.
pub const COVERAGE_AREA_TOLERANCE_M2: f64 = 1e-6;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    PartialBuilding,
    RepresentativeStudy,
    WholeBuilding,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    Mapped,
    ContextOnly,
    Excluded,
    Unresolved,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalyticalObjectKind {
    Zone,
    Surface,
    Window,
    Overhang,
    DetachedShading,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnalyticalTarget {
    pub design_id: String,
    pub object_id: String,
    pub kind: AnalyticalObjectKind,
    pub model_input_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeMapping {
    pub occurrence_id: String,
    pub status: MappingStatus,
    /// Context may explicitly map to detached analytical shading. Display-only
    /// context has no targets. Other non-mapped states never carry targets.
    pub targets: Vec<AnalyticalTarget>,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionStatus {
    Documented,
    Measured,
    Assumed,
    Synthetic,
    Missing,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssumptionLink {
    Study,
    ReferenceOccurrence {
        occurrence_id: String,
    },
    ThermalObject {
        design_id: String,
        object_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeAssumption {
    pub id: String,
    pub link: AssumptionLink,
    /// JSON Pointer into a thermal object for ThermalObject links; descriptive
    /// field text for study/reference links. Never interpreted as an edit.
    pub field: String,
    pub status: AssumptionStatus,
    pub value: Option<String>,
    pub units: String,
    pub source: String,
    pub rationale: String,
    pub affected_scope: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationMeasure {
    LocalLengthX,
    LocalLengthY,
    Height,
    FloorArea,
    Volume,
    ExteriorEnvelopeGrossArea,
    OpeningArea,
    NorthRotation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScopeUnit {
    Metres,
    SquareMetres,
    CubicMetres,
    DegreesClockwise,
}

impl ReconciliationMeasure {
    pub fn unit(self) -> ScopeUnit {
        match self {
            Self::LocalLengthX | Self::LocalLengthY | Self::Height => ScopeUnit::Metres,
            Self::FloorArea | Self::ExteriorEnvelopeGrossArea | Self::OpeningArea => {
                ScopeUnit::SquareMetres
            }
            Self::Volume => ScopeUnit::CubicMetres,
            Self::NorthRotation => ScopeUnit::DegreesClockwise,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceMeasurement {
    /// Explicit measured or documented value. Never populated from an AABB.
    pub value: f64,
    pub unit: ScopeUnit,
    pub source: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScopeReconciliation {
    pub id: String,
    pub design_id: String,
    pub occurrence_ids: Vec<String>,
    /// Zone IDs; NorthRotation instead requires the sole ID model:building.
    pub analytical_object_ids: Vec<String>,
    pub measure: ReconciliationMeasure,
    pub source_measurement: Option<SourceMeasurement>,
    /// Required even when source measurement is unknown; explains the selected
    /// source scope and the abstraction, never implies automatic equivalence.
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScopeCoverage {
    Unknown {
        reason: String,
    },
    /// Source measured floor areas, not a percentage of mesh triangles or AABBs.
    DeclaredFloorArea {
        included_m2: f64,
        whole_building_m2: f64,
        source: String,
        method: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSectionAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSectionKeep {
    Above,
    Below,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceReportSection {
    pub axis: ReferenceSectionAxis,
    pub offset_m: f64,
    pub keep: ReferenceSectionKeep,
}

/// Deterministic orthographic figure definition, bound to admitted source and
/// one accepted analytical design. Display settings never alter physics.
/// Report generation shows the full mapped/reference scope in this frame;
/// transient viewport hiding and local selection are intentionally not copied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceReportView {
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub design_id: String,
    pub model_input_sha256: String,
    pub reference_revision_sha256: String,
    pub reference_geometry_sha256: String,
    pub eye_m: [f64; 3],
    pub target_m: [f64; 3],
    pub up: [f64; 3],
    pub vertical_span_m: f64,
    pub aspect_ratio: f64,
    pub section: Option<ReferenceReportSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArchitecturalScope {
    pub schema_version: String,
    pub title: String,
    pub purpose: String,
    pub scope_kind: ScopeKind,
    pub reference_revision_sha256: String,
    pub reference_geometry_sha256: String,
    pub reference_options_sha256: String,
    pub design_model_sha256: BTreeMap<String, String>,
    pub mappings: Vec<ScopeMapping>,
    /// Human-readable scope inventories include storeys, spaces, envelope,
    /// context, systems and boundary-condition consequences as applicable.
    pub included_scope: Vec<String>,
    pub excluded_scope: Vec<String>,
    pub boundary_consequences: String,
    pub assumptions: Vec<ScopeAssumption>,
    pub reconciliations: Vec<ScopeReconciliation>,
    pub coverage: ScopeCoverage,
    /// Required for a whole-building claim; never synthesized by the importer.
    pub whole_building_review: Option<String>,
    /// Omission preserves the canonical representation of historical scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub report_views: Vec<ReferenceReportView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationResult {
    pub id: String,
    pub design_id: String,
    pub unit: ScopeUnit,
    pub source_value: Option<f64>,
    pub analytical_value: f64,
    pub analytical_minus_source: Option<f64>,
    /// None for an unknown or zero source denominator.
    pub relative_difference_percent: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScopeRelationship {
    Current,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeIssue {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScopeAssessment {
    pub schema_version: String,
    pub scope_sha256: String,
    pub report_binding_sha256: String,
    pub relationship: ScopeRelationship,
    pub stale_reasons: Vec<ScopeIssue>,
    pub coverage_percent: Option<f64>,
    pub reconciliations: Vec<ReconciliationResult>,
    pub unresolved_mapping_count: usize,
    pub missing_assumption_count: usize,
    pub disclosure: String,
}

fn bounded_list(values: &[String], field: &str, limit: usize) -> Result<()> {
    if values.len() > limit {
        return Err(invalid(field, "List exceeds the scope resource limit"));
    }
    for value in values {
        text(field, value)?;
    }
    Ok(())
}
fn unique(values: &[String], field: &str) -> Result<()> {
    bounded_list(values, field, MAX_SCOPE_MAPPINGS)?;
    if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        return Err(invalid(field, "Duplicate identifiers are not allowed"));
    }
    Ok(())
}
pub fn validate_declaration(scope: &ArchitecturalScope) -> Result<()> {
    if scope.schema_version != ARCHITECTURAL_SCOPE_SCHEMA {
        return Err(invalid(
            "scope.schema_version",
            "Unsupported architectural scope schema",
        ));
    }
    for (field, value) in [
        ("title", &scope.title),
        ("purpose", &scope.purpose),
        ("boundary_consequences", &scope.boundary_consequences),
    ] {
        text(field, value)?;
    }
    for (field, value) in [
        (
            "reference_revision_sha256",
            &scope.reference_revision_sha256,
        ),
        (
            "reference_geometry_sha256",
            &scope.reference_geometry_sha256,
        ),
        ("reference_options_sha256", &scope.reference_options_sha256),
    ] {
        hash(field, value)?;
    }
    if scope.design_model_sha256.is_empty() || scope.design_model_sha256.len() > 2 {
        return Err(invalid(
            "scope.designs",
            "Bind one or two explicitly selected designs",
        ));
    }
    for (design, value) in &scope.design_model_sha256 {
        text("scope.design_id", design)?;
        hash("scope.model_input_sha256", value)?;
    }
    if scope.report_views.len() > MAX_REFERENCE_REPORT_VIEWS {
        return Err(invalid(
            "scope.report_views",
            "At most eight portable orthographic report views",
        ));
    }
    let mut report_ids = BTreeSet::new();
    for view in &scope.report_views {
        if view.schema_version != REFERENCE_REPORT_VIEW_SCHEMA {
            return Err(invalid(
                "report_view.schema_version",
                "Unsupported report view definition",
            ));
        }
        text("report_view.id", &view.id)?;
        text("report_view.title", &view.title)?;
        if view.id.len() > 128 || view.title.len() > 256 || !report_ids.insert(&view.id) {
            return Err(invalid(
                "report_view.id",
                "Use unique IDs up to 128 bytes and titles up to 256 bytes",
            ));
        }
        if view.reference_revision_sha256 != scope.reference_revision_sha256
            || view.reference_geometry_sha256 != scope.reference_geometry_sha256
            || scope.design_model_sha256.get(&view.design_id) != Some(&view.model_input_sha256)
        {
            return Err(invalid("report_view.binding", "Figure must bind the scope's exact reference revision, geometry and declared analytical design"));
        }
        let valid_point = |v: &[f64; 3]| v.iter().all(|n| n.is_finite() && n.abs() <= 1e9);
        let forward = std::array::from_fn::<_, 3, _>(|i| view.target_m[i] - view.eye_m[i]);
        let length2 = |v: [f64; 3]| v.iter().map(|n| n * n).sum::<f64>();
        let cross = [
            forward[1] * view.up[2] - forward[2] * view.up[1],
            forward[2] * view.up[0] - forward[0] * view.up[2],
            forward[0] * view.up[1] - forward[1] * view.up[0],
        ];
        if !valid_point(&view.eye_m)
            || !valid_point(&view.target_m)
            || !valid_point(&view.up)
            || length2(forward) <= 1e-18
            || length2(view.up) <= 1e-18
            || length2(cross) <= 1e-18 * length2(forward) * length2(view.up)
            || !view.vertical_span_m.is_finite()
            || !(1e-6..=1e9).contains(&view.vertical_span_m)
            || !view.aspect_ratio.is_finite()
            || !(0.25..=4.).contains(&view.aspect_ratio)
        {
            return Err(invalid("report_view.camera", "Use finite noncoincident eye/target, a nonparallel up vector, vertical span 0.000001–1000000000 m and aspect ratio 0.25–4"));
        }
        if view
            .section
            .as_ref()
            .is_some_and(|s| !s.offset_m.is_finite() || s.offset_m.abs() > 1e9)
        {
            return Err(invalid(
                "report_view.section",
                "Section coordinate must be finite within ±1000000000 m",
            ));
        }
    }
    bounded_list(&scope.included_scope, "scope.included_scope", 256)?;
    bounded_list(&scope.excluded_scope, "scope.excluded_scope", 256)?;
    if scope.included_scope.is_empty() {
        return Err(invalid(
            "scope.included_scope",
            "Declare what the study includes",
        ));
    }
    if scope.mappings.len() > MAX_SCOPE_MAPPINGS
        || scope.assumptions.len() > MAX_SCOPE_ASSUMPTIONS
        || scope.reconciliations.len() > MAX_SCOPE_RECONCILIATIONS
    {
        return Err(invalid(
            "scope",
            "Scope records exceed the declared resource bounds",
        ));
    }
    // Count without allocating another potentially large serialized copy.
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("Scope size overflow"))?;
            if self.0 > MAX_SCOPE_BYTES {
                return Err(std::io::Error::other("Scope exceeds 8 MiB"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), scope)
        .map_err(|_| invalid("scope", "Scope exceeds 8 MiB or cannot be serialized"))?;
    let mut occurrences = BTreeSet::new();
    for mapping in &scope.mappings {
        text("mapping.occurrence_id", &mapping.occurrence_id)?;
        text("mapping.rationale", &mapping.rationale)?;
        if !occurrences.insert(&mapping.occurrence_id) {
            return Err(invalid(
                "mapping.occurrence_id",
                "Each occurrence has one explicit mapping state",
            ));
        }
        if mapping.targets.len() > 64 {
            return Err(invalid(
                "mapping.targets",
                "At most 64 targets per occurrence",
            ));
        }
        if (mapping.status == MappingStatus::Mapped && mapping.targets.is_empty())
            || (matches!(
                mapping.status,
                MappingStatus::Excluded | MappingStatus::Unresolved
            ) && !mapping.targets.is_empty())
        {
            return Err(invalid("mapping.targets", "Mapped occurrences require targets; excluded/unresolved occurrences cannot have targets"));
        }
        let mut bindings = BTreeSet::new();
        for target in &mapping.targets {
            text("mapping.object_id", &target.object_id)?;
            hash("mapping.model_input_sha256", &target.model_input_sha256)?;
            if scope.design_model_sha256.get(&target.design_id) != Some(&target.model_input_sha256)
            {
                return Err(invalid(
                    "mapping.design_id",
                    "Target must bind the selected design and its exact accepted model identity",
                ));
            }
            if !bindings.insert((&target.design_id, &target.object_id)) {
                return Err(invalid(
                    "mapping.targets",
                    "Duplicate design/object binding for an occurrence",
                ));
            }
            if mapping.status == MappingStatus::ContextOnly
                && target.kind != AnalyticalObjectKind::DetachedShading
            {
                return Err(invalid(
                    "mapping.targets",
                    "Context-only geometry can map only to explicit detached analytical shading",
                ));
            }
        }
    }
    let mut assumptions = BTreeSet::new();
    for a in &scope.assumptions {
        for (f, v) in [
            ("id", &a.id),
            ("field", &a.field),
            ("units", &a.units),
            ("source", &a.source),
            ("rationale", &a.rationale),
            ("affected_scope", &a.affected_scope),
        ] {
            text(f, v)?;
        }
        if !assumptions.insert(&a.id) {
            return Err(invalid("assumption.id", "Duplicate assumption ID"));
        }
        if let Some(value) = &a.value {
            text("assumption.value", value)?;
        }
        let absent = matches!(
            a.status,
            AssumptionStatus::Missing | AssumptionStatus::NotApplicable
        );
        if absent != a.value.is_none() {
            return Err(invalid("assumption.value", "Missing/not-applicable values must be absent; all other statuses require a declared value"));
        }
        if let AssumptionLink::ThermalObject {
            design_id,
            object_id,
        } = &a.link
        {
            if !scope.design_model_sha256.contains_key(design_id) {
                return Err(invalid("assumption.design_id", "Unknown bound design"));
            }
            text("assumption.object_id", object_id)?;
            if !a.field.starts_with('/') {
                return Err(invalid(
                    "assumption.field",
                    "Use a JSON Pointer such as /width_m into the linked thermal object",
                ));
            }
        }
    }
    let mut reconciliations = BTreeSet::new();
    for r in &scope.reconciliations {
        text("reconciliation.id", &r.id)?;
        text("reconciliation.method", &r.method)?;
        if !reconciliations.insert(&r.id) {
            return Err(invalid("reconciliation.id", "Duplicate reconciliation ID"));
        }
        if !scope.design_model_sha256.contains_key(&r.design_id) {
            return Err(invalid("reconciliation.design_id", "Unknown bound design"));
        }
        unique(&r.occurrence_ids, "reconciliation.occurrence_ids")?;
        unique(
            &r.analytical_object_ids,
            "reconciliation.analytical_object_ids",
        )?;
        if r.occurrence_ids.is_empty() || r.analytical_object_ids.is_empty() {
            return Err(invalid(
                "reconciliation",
                "Declare both source occurrences and analytical objects",
            ));
        }
        if let Some(s) = &r.source_measurement {
            if !s.value.is_finite()
                || s.value < 0.
                || s.unit != r.measure.unit()
                || (r.measure == ReconciliationMeasure::NorthRotation && s.value >= 360.)
            {
                return Err(invalid("reconciliation.source_measurement", "Use a finite nonnegative value in the declared units; orientation is [0,360) degrees"));
            }
            text("reconciliation.source", &s.source)?;
            text("reconciliation.source_method", &s.method)?;
        }
    }
    match &scope.coverage {
        ScopeCoverage::Unknown { reason } => text("coverage.reason", reason)?,
        ScopeCoverage::DeclaredFloorArea {
            included_m2,
            whole_building_m2,
            source,
            method,
        } => {
            if !included_m2.is_finite()
                || !whole_building_m2.is_finite()
                || *included_m2 < 0.
                || *whole_building_m2 <= 0.
                || included_m2 > whole_building_m2
            {
                return Err(invalid("coverage", "Coverage requires finite source areas, a positive denominator, and included area within the whole-building denominator"));
            }
            text("coverage.source", source)?;
            text("coverage.method", method)?;
        }
    }
    if let Some(review) = &scope.whole_building_review {
        text("whole_building_review", review)?;
    }
    if scope.scope_kind == ScopeKind::WholeBuilding {
        let complete_area = matches!(&scope.coverage, ScopeCoverage::DeclaredFloorArea { included_m2, whole_building_m2, .. }
            if (included_m2 - whole_building_m2).abs() <= COVERAGE_AREA_TOLERANCE_M2);
        if !complete_area
            || scope.whole_building_review.is_none()
            || !scope.excluded_scope.is_empty()
            || scope.mappings.iter().any(|m| {
                matches!(
                    m.status,
                    MappingStatus::Excluded | MappingStatus::Unresolved
                )
            })
        {
            return Err(invalid("scope.scope_kind", "Whole-building scope requires a complete measured-area denominator, explicit review, and no excluded/unresolved architecture; use partial-building or representative study otherwise"));
        }
    }
    Ok(())
}

fn target_exists(target: &AnalyticalTarget, m: &ValidatedThermalModel) -> bool {
    match target.kind {
        AnalyticalObjectKind::Zone => m.model.zones.iter().any(|z| z.id == target.object_id),
        AnalyticalObjectKind::Surface => m.surfaces.iter().any(|s| s.id == target.object_id),
        AnalyticalObjectKind::Window => m
            .model
            .zones
            .iter()
            .flat_map(|z| &z.windows)
            .any(|w| w.id == target.object_id),
        AnalyticalObjectKind::Overhang => m
            .model
            .zones
            .iter()
            .flat_map(|z| &z.windows)
            .any(|w| w.overhang.is_some() && format!("overhang:{}", w.id) == target.object_id),
        AnalyticalObjectKind::DetachedShading => m
            .model
            .detached_shading
            .iter()
            .any(|s| s.id == target.object_id),
    }
}

fn analytical_value(row: &ScopeReconciliation, model: &ValidatedThermalModel) -> Result<f64> {
    use ReconciliationMeasure::*;
    if row.measure == NorthRotation {
        if row.analytical_object_ids != ["model:building"] {
            return Err(invalid(
                "reconciliation.analytical_object_ids",
                "North rotation must reference model:building only",
            ));
        }
        return Ok(model.model.north_rotation_deg.rem_euclid(360.));
    }
    let zones = row
        .analytical_object_ids
        .iter()
        .map(|id| {
            model
                .model
                .zones
                .iter()
                .find(|z| z.id == *id)
                .ok_or_else(|| {
                    invalid(
                        "reconciliation.analytical_object_ids",
                        format!("Unknown analytical zone {id}"),
                    )
                })
        })
        .collect::<Result<Vec<_>>>()?;
    if matches!(row.measure, LocalLengthX | LocalLengthY | Height) && zones.len() != 1 {
        return Err(invalid(
            "reconciliation.analytical_object_ids",
            "A length measurement requires one zone; a collection is not a bounding-box equivalent",
        ));
    }
    let value = match row.measure {
        LocalLengthX => zones[0].size_m[0],
        LocalLengthY => zones[0].size_m[1],
        Height => zones[0].size_m[2],
        FloorArea => zones.iter().map(|z| z.size_m[0] * z.size_m[1]).sum(),
        Volume => zones.iter().map(|z| z.size_m.iter().product::<f64>()).sum(),
        OpeningArea => zones
            .iter()
            .flat_map(|z| &z.windows)
            .map(|w| w.width_m * w.height_m)
            .sum(),
        ExteriorEnvelopeGrossArea => zones
            .iter()
            .flat_map(|z| &z.surfaces)
            .filter(|s| matches!(s.boundary, ThermalBoundary::Outdoors))
            .map(|s| {
                model
                    .surfaces
                    .iter()
                    .find(|v| v.id == s.id)
                    .map(|v| v.gross_area_m2)
                    .ok_or_else(|| {
                        invalid("reconciliation", "Validated surface geometry is missing")
                    })
            })
            .collect::<Result<Vec<_>>>()?
            .iter()
            .sum(),
        NorthRotation => unreachable!(),
    };
    if !value.is_finite() {
        return Err(invalid(
            "reconciliation",
            "Analytical measurement is not finite",
        ));
    }
    Ok(value)
}

/// Historical assessment is non-mutating. A stale scope remains inspectable;
/// no target or source identity is silently rewritten. This checks metadata and
/// model relationships, not imported chunk integrity or scientific accuracy.
pub fn assess_scope(
    scope: &ArchitecturalScope,
    reference: &ReferenceModel,
    designs: &BTreeMap<String, ValidatedThermalModel>,
) -> Result<ScopeAssessment> {
    validate_declaration(scope)?;
    validate_manifest(reference).map_err(|e| invalid("scope.reference", e.to_string()))?;
    let mut reasons = Vec::new();
    let mut stale = |field: &str, message: String| {
        reasons.push(ScopeIssue {
            field: field.into(),
            message,
        })
    };
    for (field, bound, current) in [
        (
            "reference_revision_sha256",
            &scope.reference_revision_sha256,
            reference.revision_sha256.clone(),
        ),
        (
            "reference_geometry_sha256",
            &scope.reference_geometry_sha256,
            reference.geometry_sha256.clone(),
        ),
        (
            "reference_options_sha256",
            &scope.reference_options_sha256,
            canonical_hash(&reference.options)?,
        ),
    ] {
        if bound != &current {
            stale(field, "Reference source, geometry or import transform changed; review mapping before rebinding".into());
        }
    }
    let ids = reference
        .occurrences
        .iter()
        .map(|o| o.id.as_str())
        .collect::<BTreeSet<_>>();
    let objects = designs
        .iter()
        .map(|(id, m)| Ok((id.as_str(), thermal_object_snapshots(&m.model)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    for (design, bound) in &scope.design_model_sha256 {
        if designs.get(design).map(|m| &m.model_input_sha256) != Some(bound) {
            stale(
                "design_model_sha256",
                format!("Design {design} is missing or its accepted scientific inputs changed"),
            );
        }
    }
    for m in &scope.mappings {
        if !ids.contains(m.occurrence_id.as_str()) {
            stale(
                "mapping.occurrence_id",
                format!(
                    "Source occurrence {} is absent from this revision",
                    m.occurrence_id
                ),
            );
        }
        for t in &m.targets {
            if !designs
                .get(&t.design_id)
                .is_some_and(|d| target_exists(t, d))
            {
                stale(
                    "mapping.targets",
                    format!(
                        "Design {} has no {:?} object {}",
                        t.design_id, t.kind, t.object_id
                    ),
                );
            }
        }
    }
    for a in &scope.assumptions {
        match &a.link {
            AssumptionLink::ReferenceOccurrence { occurrence_id }
                if !ids.contains(occurrence_id.as_str()) =>
            {
                stale(
                    "assumption.link",
                    format!("Assumption {} links a missing source occurrence", a.id),
                )
            }
            AssumptionLink::ThermalObject {
                design_id,
                object_id,
            } => {
                match objects
                    .get(design_id.as_str())
                    .and_then(|m| m.get(object_id))
                {
                    None => stale(
                        "assumption.link",
                        format!("Assumption {} links a missing analytical object", a.id),
                    ),
                    Some(snapshot) => {
                        // Semantic lock snapshots additionally include dependent
                        // geometry. Their first object is the authoritative input.
                        let object = if object_id.starts_with("overhang:") {
                            snapshot.get(5).unwrap_or(snapshot)
                        } else if snapshot.is_array() {
                            snapshot.get(0).unwrap_or(snapshot)
                        } else {
                            snapshot
                        };
                        if object.pointer(&a.field).is_none() {
                            stale(
                                "assumption.field",
                                format!(
                                    "Assumption {} links a missing input field {}",
                                    a.id, a.field
                                ),
                            );
                        }
                    }
                }
            }
            _ => (),
        }
    }
    for r in &scope.reconciliations {
        if r.occurrence_ids.iter().any(|id| !ids.contains(id.as_str())) {
            stale(
                "reconciliation.occurrence_ids",
                format!("Reconciliation {} links a missing source occurrence", r.id),
            );
        }
        if let Some(model) = designs.get(&r.design_id) {
            if let Err(e) = analytical_value(r, model) {
                stale("reconciliation", e.to_string());
            }
        }
    }
    if scope.scope_kind == ScopeKind::WholeBuilding {
        let mapped = scope
            .mappings
            .iter()
            .map(|m| m.occurrence_id.as_str())
            .collect::<BTreeSet<_>>();
        if ids != mapped {
            stale("scope.scope_kind", "Whole-building review must explicitly classify every admitted occurrence, including grouping nodes".into());
        }
        for (design, model) in designs
            .iter()
            .filter(|(id, _)| scope.design_model_sha256.contains_key(*id))
        {
            for zone in &model.model.zones {
                if !scope.mappings.iter().flat_map(|m| &m.targets).any(|t| {
                    t.design_id == *design
                        && t.object_id == zone.id
                        && t.kind == AnalyticalObjectKind::Zone
                }) {
                    stale("scope.scope_kind", format!("Whole-building review has no source mapping for zone {} in design {design}", zone.id));
                }
            }
            let all_zones = model
                .model
                .zones
                .iter()
                .map(|z| z.id.as_str())
                .collect::<BTreeSet<_>>();
            let included = match &scope.coverage {
                ScopeCoverage::DeclaredFloorArea { included_m2, .. } => *included_m2,
                _ => 0.,
            };
            if !scope.reconciliations.iter().any(|r| {
                r.design_id == *design
                    && r.measure == ReconciliationMeasure::FloorArea
                    && r.analytical_object_ids
                        .iter()
                        .map(String::as_str)
                        .collect::<BTreeSet<_>>()
                        == all_zones
                    && r.source_measurement
                        .as_ref()
                        .is_some_and(|s| (s.value - included).abs() <= COVERAGE_AREA_TOLERANCE_M2)
            }) {
                stale("scope.scope_kind", format!("Whole-building review requires an explicit source floor-area reconciliation covering every zone in design {design}"));
            }
        }
    }
    let relationship = if reasons.is_empty() {
        ScopeRelationship::Current
    } else {
        ScopeRelationship::Stale
    };
    // Do not present new geometry reconciled under an old binding.
    let reconciliations = if relationship == ScopeRelationship::Current {
        scope
            .reconciliations
            .iter()
            .map(|r| {
                let value = analytical_value(r, &designs[&r.design_id])?;
                let source = r.source_measurement.as_ref().map(|s| s.value);
                let delta = source.map(|s| {
                    if r.measure == ReconciliationMeasure::NorthRotation {
                        (value - s + 180.).rem_euclid(360.) - 180.
                    } else {
                        value - s
                    }
                });
                let relative = if r.measure == ReconciliationMeasure::NorthRotation { None }
                    else { source.filter(|s| *s != 0.).zip(delta).map(|(s, d)| 100. * (d / s)) };
                if relative.is_some_and(|v| !v.is_finite()) || delta.is_some_and(|v| !v.is_finite()) {
                    return Err(invalid("reconciliation.source_measurement", "Declared measurement produces a nonfinite comparison; review its scale and units"));
                }
                Ok(ReconciliationResult {
                    id: r.id.clone(),
                    design_id: r.design_id.clone(),
                    unit: r.measure.unit(),
                    source_value: source,
                    analytical_value: value,
                    analytical_minus_source: delta,
                    relative_difference_percent: relative,
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let coverage_percent = match &scope.coverage {
        ScopeCoverage::Unknown { .. } => None,
        ScopeCoverage::DeclaredFloorArea {
            included_m2,
            whole_building_m2,
            ..
        } => Some(100. * (included_m2 / whole_building_m2)),
    };
    let scope_sha256 = canonical_hash(&(ARCHITECTURAL_SCOPE_SCHEMA, scope))?;
    let current_models = designs
        .iter()
        .filter(|(id, _)| scope.design_model_sha256.contains_key(*id))
        .map(|(id, m)| (id, &m.model_input_sha256))
        .collect::<BTreeMap<_, _>>();
    let report_binding_sha256 = canonical_hash(&(
        SCOPE_ASSESSMENT_SCHEMA,
        &scope_sha256,
        &reference.revision_sha256,
        &reference.geometry_sha256,
        current_models,
        &reconciliations,
        &reasons,
    ))?;
    let classified = scope
        .mappings
        .iter()
        .map(|m| m.occurrence_id.as_str())
        .collect::<BTreeSet<_>>();
    Ok(ScopeAssessment { schema_version: SCOPE_ASSESSMENT_SCHEMA.into(), scope_sha256, report_binding_sha256,
        relationship, stale_reasons: reasons, coverage_percent, reconciliations,
        unresolved_mapping_count: scope.mappings.iter().filter(|m| m.status == MappingStatus::Unresolved).count()
            + ids.difference(&classified).count(),
        missing_assumption_count: scope.assumptions.iter().filter(|a| a.status == AssumptionStatus::Missing).count(),
        disclosure: "Scope and source measurements are declared workflow evidence, not physical validation or whole-building equivalence. Reference display materials do not define thermal inputs. Unknown coverage remains unknown; subset energy must not be extrapolated to the whole building.".into() })
}

/// Accept a scope only against its exact current reference and accepted models.
/// Draft reports may retain unresolved assumptions with visible counts.
pub fn validate_scope(
    scope: &ArchitecturalScope,
    reference: &ReferenceModel,
    designs: &BTreeMap<String, ValidatedThermalModel>,
) -> Result<ScopeAssessment> {
    let assessment = assess_scope(scope, reference, designs)?;
    if let Some(issue) = assessment.stale_reasons.first() {
        return Err(invalid(&issue.field, &issue.message));
    }
    Ok(assessment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::{
        synthetic_energy_calendar, synthetic_four_zone_model, validate_thermal_model,
    };
    use crate::reference::{admit_glb, ReferenceImportOptions};
    use serde_json::json;

    fn source(options: ReferenceImportOptions) -> ReferenceModel {
        // Original inert triangle fixture. It establishes contract mechanics,
        // not an architectural equivalence to the synthetic thermal building.
        let mut bin = Vec::new();
        for v in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            .iter()
            .flatten()
        {
            bin.extend(v.to_le_bytes());
        }
        let doc = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],
            "bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
            "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
        let mut json = serde_json::to_vec(&doc).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut glb = Vec::new();
        glb.extend(0x4654_6c67u32.to_le_bytes());
        glb.extend(2u32.to_le_bytes());
        glb.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
        glb.extend((json.len() as u32).to_le_bytes());
        glb.extend(0x4e4f_534au32.to_le_bytes());
        glb.extend(json);
        glb.extend((bin.len() as u32).to_le_bytes());
        glb.extend(0x004e_4942u32.to_le_bytes());
        glb.extend(bin);
        admit_glb(&glb, &options, |_| true).unwrap().manifest
    }

    fn fixture() -> (
        ArchitecturalScope,
        ReferenceModel,
        BTreeMap<String, ValidatedThermalModel>,
    ) {
        let reference = source(ReferenceImportOptions::default());
        let model = validate_thermal_model(
            &synthetic_four_zone_model(false),
            &synthetic_energy_calendar(),
        )
        .unwrap();
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "Original synthetic contract fixture".into(),
            purpose: "Test explicit metadata; no geometry equivalence claimed".into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: reference.revision_sha256.clone(),
            reference_geometry_sha256: reference.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&reference.options).unwrap(),
            design_model_sha256: BTreeMap::from([("A".into(), model.model_input_sha256.clone())]),
            mappings: vec![ScopeMapping {
                occurrence_id: reference.occurrences[0].id.clone(),
                status: MappingStatus::Mapped,
                targets: vec![AnalyticalTarget {
                    design_id: "A".into(),
                    object_id: model.model.zones[0].id.clone(),
                    kind: AnalyticalObjectKind::Zone,
                    model_input_sha256: model.model_input_sha256.clone(),
                }],
                rationale: "Synthetic mapping for contract tests only".into(),
            }],
            included_scope: vec!["Synthetic selected zone".into()],
            excluded_scope: vec!["No whole-building coverage claim".into()],
            boundary_consequences: "Existing declared boundaries remain authoritative".into(),
            assumptions: vec![ScopeAssumption {
                id: "site-source".into(),
                link: AssumptionLink::ThermalObject {
                    design_id: "A".into(),
                    object_id: "model:building".into(),
                },
                field: "/site".into(),
                status: AssumptionStatus::Synthetic,
                value: Some("Original test location".into()),
                units: "degrees".into(),
                source: "Original fixture".into(),
                rationale: "Not inferred from geometry".into(),
                affected_scope: "All zones".into(),
            }],
            reconciliations: vec![ScopeReconciliation {
                id: "floor-area".into(),
                design_id: "A".into(),
                occurrence_ids: vec![reference.occurrences[0].id.clone()],
                analytical_object_ids: vec![model.model.zones[0].id.clone()],
                measure: ReconciliationMeasure::FloorArea,
                source_measurement: None,
                method: "Analytical rectangle area; source semantic area remains unknown".into(),
            }],
            coverage: ScopeCoverage::Unknown {
                reason: "No source floor-area denominator established".into(),
            },
            whole_building_review: None,
            report_views: vec![],
        };
        (scope, reference, BTreeMap::from([("A".into(), model)]))
    }

    #[test]
    fn analytical_values_are_rust_derived_and_unknown_source_is_not_bounds_volume() {
        let (scope, reference, models) = fixture();
        let assessed = validate_scope(&scope, &reference, &models).unwrap();
        let zone = &models["A"].model.zones[0];
        assert_eq!(assessed.relationship, ScopeRelationship::Current);
        assert_eq!(assessed.coverage_percent, None);
        assert_eq!(
            assessed.reconciliations[0].analytical_value,
            zone.size_m[0] * zone.size_m[1]
        );
        assert_eq!(assessed.reconciliations[0].source_value, None);
        assert_eq!(
            assessed.reconciliations[0].relative_difference_percent,
            None
        );
        assert_eq!(assessed.unresolved_mapping_count, 0);
    }

    #[test]
    fn duplicate_missing_and_wrong_kind_bindings_are_rejected() {
        let (scope, reference, models) = fixture();
        let mut changed = scope.clone();
        changed.mappings.push(changed.mappings[0].clone());
        assert!(validate_scope(&changed, &reference, &models).is_err());
        changed = scope.clone();
        let duplicate = changed.mappings[0].targets[0].clone();
        changed.mappings[0].targets.push(duplicate);
        assert!(validate_scope(&changed, &reference, &models).is_err());
        changed = scope.clone();
        changed.mappings[0].targets[0].object_id = "missing-object".into();
        assert!(validate_scope(&changed, &reference, &models).is_err());
        changed = scope.clone();
        changed.mappings[0].targets[0].kind = AnalyticalObjectKind::Window;
        assert!(validate_scope(&changed, &reference, &models).is_err());
        changed = scope;
        changed.assumptions[0].field = "/missing_field".into();
        assert!(validate_scope(&changed, &reference, &models).is_err());
    }

    #[test]
    fn changing_model_or_import_transform_marks_stale_without_rebinding() {
        let (scope, reference, mut models) = fixture();
        let immutable = serde_json::to_vec(&scope).unwrap();
        let mut proposed = models["A"].model.clone();
        proposed.north_rotation_deg = 12.;
        models.insert(
            "A".into(),
            validate_thermal_model(&proposed, &synthetic_energy_calendar()).unwrap(),
        );
        let assessment = assess_scope(&scope, &reference, &models).unwrap();
        assert_eq!(assessment.relationship, ScopeRelationship::Stale);
        assert!(assessment.reconciliations.is_empty());
        assert!(validate_scope(&scope, &reference, &models).is_err());
        let changed = source(ReferenceImportOptions {
            translation_m: [1., 0., 0.],
            ..Default::default()
        });
        assert_eq!(
            assess_scope(&scope, &changed, &models)
                .unwrap()
                .relationship,
            ScopeRelationship::Stale
        );
        assert_eq!(serde_json::to_vec(&scope).unwrap(), immutable);
    }

    #[test]
    fn measurements_require_correct_finite_units_and_keep_zero_percentage_undefined() {
        let (mut scope, reference, models) = fixture();
        scope.reconciliations[0].source_measurement = Some(SourceMeasurement {
            value: 0.,
            unit: ScopeUnit::SquareMetres,
            source: "Synthetic explicit measurement".into(),
            method: "Test arithmetic, not a measurement of the source mesh".into(),
        });
        assert_eq!(
            validate_scope(&scope, &reference, &models)
                .unwrap()
                .reconciliations[0]
                .relative_difference_percent,
            None
        );
        scope.reconciliations[0]
            .source_measurement
            .as_mut()
            .unwrap()
            .value = f64::INFINITY;
        assert!(validate_scope(&scope, &reference, &models).is_err());
        scope.reconciliations[0]
            .source_measurement
            .as_mut()
            .unwrap()
            .value = 20.;
        scope.reconciliations[0]
            .source_measurement
            .as_mut()
            .unwrap()
            .unit = ScopeUnit::CubicMetres;
        assert!(validate_scope(&scope, &reference, &models).is_err());
        scope.reconciliations[0]
            .source_measurement
            .as_mut()
            .unwrap()
            .unit = ScopeUnit::SquareMetres;
        let r = &validate_scope(&scope, &reference, &models)
            .unwrap()
            .reconciliations[0];
        assert_eq!(r.analytical_minus_source, Some(r.analytical_value - 20.));
        assert_eq!(
            r.relative_difference_percent,
            Some(100. * ((r.analytical_value - 20.) / 20.))
        );
    }

    #[test]
    fn whole_building_claim_requires_complete_declared_coverage_and_all_zone_reconciliation() {
        let (mut scope, reference, models) = fixture();
        scope.scope_kind = ScopeKind::WholeBuilding;
        assert!(validate_scope(&scope, &reference, &models).is_err());
        scope.excluded_scope.clear();
        scope.whole_building_review = Some("Synthetic declared coverage test only".into());
        let area = models["A"].gross_floor_area_m2;
        scope.coverage = ScopeCoverage::DeclaredFloorArea {
            included_m2: area,
            whole_building_m2: area,
            source: "Original declared fixture".into(),
            method: "Test contract; no source geometry equivalence".into(),
        };
        assert!(validate_scope(&scope, &reference, &models).is_err());
        scope.mappings[0].targets = models["A"]
            .model
            .zones
            .iter()
            .map(|z| AnalyticalTarget {
                design_id: "A".into(),
                object_id: z.id.clone(),
                kind: AnalyticalObjectKind::Zone,
                model_input_sha256: models["A"].model_input_sha256.clone(),
            })
            .collect();
        scope.reconciliations[0].analytical_object_ids = models["A"]
            .model
            .zones
            .iter()
            .map(|z| z.id.clone())
            .collect();
        scope.reconciliations[0].source_measurement = Some(SourceMeasurement {
            value: area,
            unit: ScopeUnit::SquareMetres,
            source: "Synthetic declaration".into(),
            method: "Test contract".into(),
        });
        assert_eq!(
            validate_scope(&scope, &reference, &models)
                .unwrap()
                .coverage_percent,
            Some(100.)
        );
        scope.mappings.clear();
        assert!(validate_scope(&scope, &reference, &models).is_err());
    }

    #[test]
    fn scope_edits_change_only_derived_identity_and_unresolved_remains_explicit() {
        let (mut scope, reference, models) = fixture();
        let before = serde_json::to_vec(&models).unwrap();
        let first = validate_scope(&scope, &reference, &models).unwrap();
        scope.purpose = "Revised report purpose".into();
        let second = validate_scope(&scope, &reference, &models).unwrap();
        assert_ne!(first.report_binding_sha256, second.report_binding_sha256);
        assert_eq!(serde_json::to_vec(&models).unwrap(), before);
        scope.mappings.clear();
        scope.assumptions[0].status = AssumptionStatus::Missing;
        scope.assumptions[0].value = None;
        let draft = validate_scope(&scope, &reference, &models).unwrap();
        assert_eq!(draft.unresolved_mapping_count, 1);
        assert_eq!(draft.missing_assumption_count, 1);
        let roundtrip: ArchitecturalScope =
            serde_json::from_slice(&serde_json::to_vec(&scope).unwrap()).unwrap();
        assert_eq!(roundtrip, scope);
    }

    #[test]
    fn invalid_coverage_and_scope_resource_bounds_are_rejected() {
        let (mut scope, reference, models) = fixture();
        scope.coverage = ScopeCoverage::DeclaredFloorArea {
            included_m2: 2.,
            whole_building_m2: 1.,
            source: "Fixture".into(),
            method: "Declared".into(),
        };
        assert!(validate_scope(&scope, &reference, &models).is_err());
        scope.coverage = ScopeCoverage::Unknown {
            reason: "Unknown denominator".into(),
        };
        scope.assumptions = vec![scope.assumptions[0].clone(); MAX_SCOPE_ASSUMPTIONS + 1];
        assert!(validate_scope(&scope, &reference, &models).is_err());
    }

    fn report_view(scope: &ArchitecturalScope) -> ReferenceReportView {
        ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "section-1".into(),
            title: "Synthetic north section".into(),
            design_id: "A".into(),
            model_input_sha256: scope.design_model_sha256["A"].clone(),
            reference_revision_sha256: scope.reference_revision_sha256.clone(),
            reference_geometry_sha256: scope.reference_geometry_sha256.clone(),
            eye_m: [0., -20., 4.],
            target_m: [0., 0., 4.],
            up: [0., 0., 1.],
            vertical_span_m: 12.,
            aspect_ratio: 1.5,
            section: Some(ReferenceReportSection {
                axis: ReferenceSectionAxis::Y,
                offset_m: 3.,
                keep: ReferenceSectionKeep::Below,
            }),
        }
    }

    #[test]
    fn portable_figures_bind_derived_scope_without_rewriting_legacy_or_physics() {
        let (mut scope, reference, models) = fixture();
        let legacy_bytes = serde_json::to_vec(&scope).unwrap();
        assert!(!String::from_utf8_lossy(&legacy_bytes).contains("report_views"));
        let roundtrip: ArchitecturalScope = serde_json::from_slice(&legacy_bytes).unwrap();
        assert_eq!(serde_json::to_vec(&roundtrip).unwrap(), legacy_bytes);
        let first = validate_scope(&scope, &reference, &models).unwrap();
        let physical = serde_json::to_vec(&models).unwrap();
        scope.report_views.push(report_view(&scope));
        let second = validate_scope(&scope, &reference, &models).unwrap();
        assert_ne!(first.scope_sha256, second.scope_sha256);
        assert_ne!(first.report_binding_sha256, second.report_binding_sha256);
        assert_eq!(serde_json::to_vec(&models).unwrap(), physical);
        let retained: ArchitecturalScope =
            serde_json::from_slice(&serde_json::to_vec(&scope).unwrap()).unwrap();
        assert_eq!(retained, scope);
    }

    #[test]
    fn report_camera_validation_rejects_degeneracy_resource_and_binding_errors() {
        let (mut scope, reference, models) = fixture();
        let valid = report_view(&scope);
        for change in 0..8 {
            let mut view = valid.clone();
            match change {
                0 => view.eye_m = view.target_m,
                1 => view.up = [0., 1., 0.],
                2 => view.vertical_span_m = 0.,
                3 => view.aspect_ratio = f64::NAN,
                4 => view.reference_revision_sha256 = "0".repeat(64),
                5 => view.model_input_sha256 = "0".repeat(64),
                6 => view.section.as_mut().unwrap().offset_m = f64::INFINITY,
                _ => view.schema_version = "unknown".into(),
            }
            scope.report_views = vec![view];
            assert!(
                validate_scope(&scope, &reference, &models).is_err(),
                "case {change}"
            );
        }
        scope.report_views = vec![valid.clone(); 2];
        assert!(validate_scope(&scope, &reference, &models).is_err());
        scope.report_views = vec![valid; MAX_REFERENCE_REPORT_VIEWS + 1];
        assert!(validate_scope(&scope, &reference, &models).is_err());
    }
}
