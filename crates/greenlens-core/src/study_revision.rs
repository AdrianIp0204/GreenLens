//! Immutable, derived study documentation. These records neither change physical
//! inputs nor confer scientific qualification. The runner checks retained bytes,
//! stores immutable envelopes and publishes the project manifest atomically.
use crate::architectural_scope::*;
use crate::energy::*;
use crate::reference::{validate_manifest, ReferenceModel};
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub const SCOPE_REVISION_SCHEMA: &str = "greenlens.scope-revision.v1";
pub const CARRY_FORWARD_SCHEMA: &str = "greenlens.scope-carry-forward.v1";
pub const PROVENANCE_CHECKLIST_SCHEMA: &str = "greenlens.input-provenance-checklist.v1";
pub const MAX_SCOPE_REVISIONS: usize = 64;
pub const MAX_REPORT_ISSUES: usize = 32;
pub const MAX_REVISION_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_CARRY_FORWARD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PROVENANCE_ROWS: usize = 65_536;
pub const MAX_REVIEW_ROWS: usize = 65_536;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RevisionReferenceSnapshot {
    pub asset_id: String,
    /// Exact retained file digest, checked by the runner. The canonical manifest
    /// digest cannot substitute for the original serialized bytes.
    pub manifest_sha256: String,
    pub manifest: ReferenceModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RevisionDesignSnapshot {
    pub model: ThermalModel,
    pub calendar: EnergyCalendar,
    /// A scope can be accepted before weather is assigned. Absence remains
    /// explicit and does not fabricate a runnable study.
    pub study: Option<EnergyStudy>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceStatus {
    Unreviewed,
    Documented,
    Measured,
    Assumed,
    Synthetic,
    Missing,
    NotApplicable,
    Conflicting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InputProvenanceRow {
    pub id: String,
    pub design_id: String,
    pub object: ThermalObjectRef,
    pub field_path: String,
    pub applied_value: Value,
    pub units: Option<String>,
    /// A citation in an input is declared source information, not a provenance
    /// review or proof that the applied value was obtained from that source.
    pub declared_source: Option<InputSource>,
    pub status: ProvenanceStatus,
    /// Full source declarations stay distinct from the actual applied value.
    pub source_declarations: Vec<ScopeAssumption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceChecklist {
    pub schema_version: String,
    pub rows: Vec<InputProvenanceRow>,
    pub unreviewed_count: usize,
    pub missing_count: usize,
    pub conflicting_count: usize,
    pub documentation_complete: bool,
    pub disclosure: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionChangeKind {
    Unchanged,
    Changed,
    Deleted,
    Added,
    Unresolved,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionRowKind {
    Scope,
    Reference,
    Mapping,
    Assumption,
    Reconciliation,
    ReportView,
    Input,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CarryForwardRow {
    pub id: String,
    pub kind: RevisionRowKind,
    pub key: String,
    pub change: RevisionChangeKind,
    pub requires_review: bool,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewResolution {
    /// The reviewer inspected the proposed retention/change. This does not
    /// assert source validity, physical equivalence or scientific qualification.
    Reviewed,
    /// The row is deliberately left unresolved, with an explicit explanation.
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecision {
    pub row_id: String,
    pub resolution: ReviewResolution,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CarryForwardPlan {
    pub schema_version: String,
    pub parent_revision_sha256: String,
    pub target_context_sha256: String,
    pub proposed_scope: ArchitecturalScope,
    pub rows: Vec<CarryForwardRow>,
    pub plan_sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScopeRevision {
    pub schema_version: String,
    pub revision_sha256: String,
    pub parent_revision_sha256: Option<String>,
    pub reference: RevisionReferenceSnapshot,
    pub designs: BTreeMap<String, RevisionDesignSnapshot>,
    pub scope: ArchitecturalScope,
    pub carry_forward_plan_sha256: Option<String>,
    pub review_decisions: Vec<ReviewDecision>,
    pub provenance: ProvenanceChecklist,
}

// JavaScript has one number type: a typed f64 such as 600.0 returns through
// JSON.stringify as 600. Restore only that lossless representation change in
// the derived checklist. The authoritative typed snapshots, serialization,
// historical identity algorithm and subsequent validation remain unchanged.
impl<'de> Deserialize<'de> for ScopeRevision {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        use serde::de::Error as _;
        #[derive(Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireRevision {
            schema_version: String,
            revision_sha256: String,
            parent_revision_sha256: Option<String>,
            reference: RevisionReferenceSnapshot,
            designs: BTreeMap<String, RevisionDesignSnapshot>,
            scope: ArchitecturalScope,
            carry_forward_plan_sha256: Option<String>,
            review_decisions: Vec<ReviewDecision>,
            // These checklist/row/source types deny unknown fields and reject
            // duplicate metadata keys, preserving the original reader behavior.
            provenance: ProvenanceChecklist,
        }
        let mut wire = WireRevision::deserialize(deserializer)?;
        bound(&wire, MAX_REVISION_BYTES, "revision").map_err(D::Error::custom)?;
        if wire.schema_version != SCOPE_REVISION_SCHEMA {
            return Err(D::Error::custom("Unsupported immutable scope revision"));
        }
        if wire.designs.is_empty()
            || wire.designs.len() > 2
            || wire
                .designs
                .keys()
                .any(|id| !matches!(id.as_str(), "A" | "B"))
        {
            return Err(D::Error::custom("Retain one or both supported A/B designs"));
        }
        if wire.provenance.rows.len() > MAX_PROVENANCE_ROWS
            || wire.review_decisions.len() > MAX_REVIEW_ROWS
        {
            return Err(D::Error::custom("Immutable revision exceeds its row limit"));
        }
        bound(
            &wire.provenance,
            MAX_CARRY_FORWARD_BYTES,
            "revision.provenance",
        )
        .map_err(D::Error::custom)?;
        let provenance =
            provenance_checklist(&wire.designs, &wire.scope).map_err(D::Error::custom)?;
        if wire.provenance.rows.len() == provenance.rows.len() {
            for (received, expected) in wire.provenance.rows.iter_mut().zip(&provenance.rows) {
                restore_provenance_number_representation(
                    &mut received.applied_value,
                    &expected.applied_value,
                );
            }
        }
        if wire.provenance != provenance {
            return Err(D::Error::custom(
                "Checklist does not match retained inputs and source declarations",
            ));
        }
        Ok(Self {
            schema_version: wire.schema_version,
            revision_sha256: wire.revision_sha256,
            parent_revision_sha256: wire.parent_revision_sha256,
            reference: wire.reference,
            designs: wire.designs,
            scope: wire.scope,
            carry_forward_plan_sha256: wire.carry_forward_plan_sha256,
            review_decisions: wire.review_decisions,
            provenance,
        })
    }
}

fn restore_provenance_number_representation(received: &mut Value, expected: &Value) {
    let (Value::Number(actual), Value::Number(required)) = (&*received, expected) else {
        return;
    };
    // No approximate numeric equality, floating-to-floating correction,
    // wide-integer rounding or normalization of any other field is allowed.
    const JS_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
    if required.is_f64()
        && actual.as_i64().is_some_and(|n| {
            (-JS_MAX_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&n)
                && Some(n as f64) == required.as_f64()
        })
    {
        *received = expected.clone();
    }
}

fn bound(value: &impl Serialize, maximum: usize, field: &str) -> Result<()> {
    struct Counter {
        count: usize,
        maximum: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.count = self
                .count
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("Serialized size overflow"))?;
            if self.count > self.maximum {
                return Err(std::io::Error::other("Serialized resource bound exceeded"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter { count: 0, maximum }, value).map_err(|_| {
        invalid(
            field,
            format!("Serialized record exceeds its {maximum}-byte limit"),
        )
    })
}

pub fn validate_revision_context(
    reference: &RevisionReferenceSnapshot,
    designs: &BTreeMap<String, RevisionDesignSnapshot>,
) -> Result<BTreeMap<String, ValidatedThermalModel>> {
    bound(
        &(reference, designs),
        MAX_REVISION_BYTES,
        "revision.context",
    )?;
    text("revision.asset_id", &reference.asset_id)?;
    if reference.asset_id.len() > 128 {
        return Err(invalid(
            "revision.asset_id",
            "Asset identity exceeds 128 bytes",
        ));
    }
    hash("revision.manifest_sha256", &reference.manifest_sha256)?;
    validate_manifest(&reference.manifest)
        .map_err(|e| invalid("revision.reference", e.to_string()))?;
    if designs.is_empty()
        || designs.len() > 2
        || designs.keys().any(|id| !matches!(id.as_str(), "A" | "B"))
    {
        return Err(invalid(
            "revision.designs",
            "Retain one or both supported A/B designs",
        ));
    }
    designs
        .iter()
        .map(|(id, snapshot)| {
            if let Some(study) = &snapshot.study {
                study.validate(&snapshot.model)?;
                if study.calendar != snapshot.calendar {
                    return Err(invalid(
                        "revision.calendar",
                        "Study and retained model calendars differ",
                    ));
                }
            }
            Ok((
                id.clone(),
                validate_thermal_model(&snapshot.model, &snapshot.calendar)?,
            ))
        })
        .collect()
}

pub fn revision_context_sha256(
    reference: &RevisionReferenceSnapshot,
    designs: &BTreeMap<String, RevisionDesignSnapshot>,
) -> Result<String> {
    validate_revision_context(reference, designs)?;
    canonical_hash(&("greenlens.scope-revision-context.v1", reference, designs))
}

impl ScopeRevision {
    pub fn identity(&self) -> Result<String> {
        let mut value = self.clone();
        value.revision_sha256.clear();
        canonical_hash(&(SCOPE_REVISION_SCHEMA, value))
    }
}
impl CarryForwardPlan {
    pub fn identity(&self) -> Result<String> {
        let mut value = self.clone();
        value.plan_sha256.clear();
        canonical_hash(&(CARRY_FORWARD_SCHEMA, value))
    }
}

pub fn validate_revision(revision: &ScopeRevision) -> Result<()> {
    bound(revision, MAX_REVISION_BYTES, "revision")?;
    if revision.schema_version != SCOPE_REVISION_SCHEMA {
        return Err(invalid(
            "revision.schema_version",
            "Unsupported immutable scope revision",
        ));
    }
    hash("revision.revision_sha256", &revision.revision_sha256)?;
    for value in [
        &revision.parent_revision_sha256,
        &revision.carry_forward_plan_sha256,
    ]
    .into_iter()
    .flatten()
    {
        hash("revision.review_binding", value)?;
    }
    if revision.parent_revision_sha256.is_some() != revision.carry_forward_plan_sha256.is_some()
        || revision.parent_revision_sha256.as_ref() == Some(&revision.revision_sha256)
    {
        return Err(invalid("revision.parent", "A carried revision requires an exact parent and review plan; self-parenting is invalid"));
    }
    validate_decisions(&revision.review_decisions)?;
    if revision.parent_revision_sha256.is_none() && !revision.review_decisions.is_empty() {
        return Err(invalid(
            "revision.review",
            "Initial scope has no carry-forward decisions",
        ));
    }
    let models = validate_revision_context(&revision.reference, &revision.designs)?;
    validate_scope(&revision.scope, &revision.reference.manifest, &models)?;
    if revision.provenance != provenance_checklist(&revision.designs, &revision.scope)? {
        return Err(invalid(
            "revision.provenance",
            "Checklist does not match retained inputs and source declarations",
        ));
    }
    if revision.identity()? != revision.revision_sha256 {
        return Err(invalid(
            "revision.revision_sha256",
            "Immutable revision content differs from its identity",
        ));
    }
    Ok(())
}

/// Preserve the original v1 scope without relabeling historical run evidence.
pub fn initial_revision(
    scope: ArchitecturalScope,
    reference: RevisionReferenceSnapshot,
    designs: BTreeMap<String, RevisionDesignSnapshot>,
) -> Result<ScopeRevision> {
    let models = validate_revision_context(&reference, &designs)?;
    validate_scope(&scope, &reference.manifest, &models)?;
    let provenance = provenance_checklist(&designs, &scope)?;
    let mut revision = ScopeRevision {
        schema_version: SCOPE_REVISION_SCHEMA.into(),
        revision_sha256: String::new(),
        parent_revision_sha256: None,
        reference,
        designs,
        scope,
        carry_forward_plan_sha256: None,
        review_decisions: vec![],
        provenance,
    };
    bound(&revision, MAX_REVISION_BYTES, "revision")?;
    revision.revision_sha256 = revision.identity()?;
    validate_revision(&revision)?;
    Ok(revision)
}

/// Only generated coordinates, identities, topology and parent relationships
/// enter this signature. COP, materials and labels are deliberately absent.
pub fn geometry_signatures(snapshot: &RevisionDesignSnapshot) -> Result<BTreeMap<String, String>> {
    let scene = thermal_scene(&snapshot.model, &snapshot.calendar)?;
    let mut result = BTreeMap::new();
    for object in &scene.objects {
        let child_geometry = if object.kind == "zone" {
            scene
                .objects
                .iter()
                .filter(|child| child.zone_id.as_ref() == Some(&object.id) && child.kind != "zone")
                .map(|child| (&child.id, &child.kind, &child.polygons_m))
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        // Boundaries are analytical interpretation, not only displayed lines.
        let boundary = snapshot
            .model
            .zones
            .iter()
            .flat_map(|z| &z.surfaces)
            .find(|surface| surface.id == object.id)
            .map(|s| &s.boundary);
        result.insert(
            object.id.clone(),
            canonical_hash(&(
                "greenlens.mapping-geometry.v1",
                &object.id,
                &object.kind,
                &object.zone_id,
                object.storey,
                &object.polygons_m,
                child_geometry,
                boundary,
            ))?,
        );
    }
    result.insert("model:building".into(), canonical_hash(&result)?);
    Ok(result)
}

struct InputObject {
    object: ThermalObjectRef,
    assumption_object: String,
    assumption_prefix: String,
    value: Value,
    source: Option<InputSource>,
}

fn input_objects(snapshot: &RevisionDesignSnapshot) -> Result<Vec<InputObject>> {
    let model = &snapshot.model;
    validate_thermal_model(model, &snapshot.calendar)?;
    let mut result = vec![];
    let mut push = |kind,
                    id: &str,
                    zone: Option<&str>,
                    assumption_id: &str,
                    prefix: &str,
                    mut value: Value,
                    excluded: &[&str],
                    source: Option<InputSource>| {
        if let Some(map) = value.as_object_mut() {
            for key in excluded {
                map.remove(*key);
            }
            for key in ["id", "name", "schema_version", "source", "sizing_source"] {
                map.remove(key);
            }
        }
        result.push(InputObject {
            object: ThermalObjectRef::new(kind, id, zone),
            assumption_object: assumption_id.into(),
            assumption_prefix: prefix.into(),
            value,
            source,
        });
    };
    push(
        ThermalObjectKind::Model,
        "model:building",
        None,
        "model:building",
        "",
        serde_json::to_value(model)?,
        &[
            "site",
            "zones",
            "materials",
            "constructions",
            "glazing",
            "curves",
            "schedules",
            "detached_shading",
        ],
        None,
    );
    push(
        ThermalObjectKind::Site,
        "site",
        None,
        "model:building",
        "/site",
        serde_json::to_value(&model.site)?,
        &[],
        None,
    );
    for material in &model.materials {
        push(
            ThermalObjectKind::Material,
            &material.id,
            None,
            &material.id,
            "",
            serde_json::to_value(material)?,
            &[],
            Some(material.source.clone()),
        );
    }
    for construction in &model.constructions {
        push(
            ThermalObjectKind::Construction,
            &construction.id,
            None,
            &construction.id,
            "",
            serde_json::to_value(construction)?,
            &[],
            None,
        );
    }
    for glazing in &model.glazing {
        push(
            ThermalObjectKind::Glazing,
            &glazing.id,
            None,
            &glazing.id,
            "",
            serde_json::to_value(glazing)?,
            &[],
            Some(glazing.source.clone()),
        );
    }
    for curve in &model.curves {
        push(
            ThermalObjectKind::Curve,
            &curve.id,
            None,
            &curve.id,
            "",
            serde_json::to_value(curve)?,
            &[],
            Some(curve.source.clone()),
        );
    }
    for schedule in &model.schedules {
        push(
            ThermalObjectKind::Schedule,
            &schedule.id,
            None,
            &schedule.id,
            "",
            serde_json::to_value(schedule)?,
            &[],
            None,
        );
    }
    for zone in &model.zones {
        push(
            ThermalObjectKind::Zone,
            &zone.id,
            Some(&zone.id),
            &zone.id,
            "",
            serde_json::to_value(zone)?,
            &["surfaces", "windows", "gains", "cooling_system"],
            None,
        );
        push(
            ThermalObjectKind::Gains,
            &zone.id,
            Some(&zone.id),
            &zone.id,
            "/gains",
            serde_json::to_value(&zone.gains)?,
            &[],
            Some(zone.gains.source.clone()),
        );
        for surface in &zone.surfaces {
            push(
                ThermalObjectKind::Surface,
                &surface.id,
                Some(&zone.id),
                &surface.id,
                "",
                serde_json::to_value(surface)?,
                &[],
                None,
            );
        }
        for window in &zone.windows {
            push(
                ThermalObjectKind::Window,
                &window.id,
                Some(&zone.id),
                &window.id,
                "",
                serde_json::to_value(window)?,
                &["overhang"],
                None,
            );
            if let Some(overhang) = &window.overhang {
                push(
                    ThermalObjectKind::Overhang,
                    &window.id,
                    Some(&zone.id),
                    &format!("overhang:{}", window.id),
                    "",
                    serde_json::to_value(overhang)?,
                    &[],
                    None,
                );
            }
        }
        if let Some(system) = &zone.cooling_system {
            push(
                ThermalObjectKind::CoolingSystem,
                &system.id,
                Some(&zone.id),
                &system.id,
                "",
                serde_json::to_value(system)?,
                &["supply_fan", "balanced_ventilation"],
                Some(system.sizing_source.clone()),
            );
            push(
                ThermalObjectKind::SupplyFan,
                &system.id,
                Some(&zone.id),
                &system.id,
                "/supply_fan",
                serde_json::to_value(&system.supply_fan)?,
                &[],
                Some(system.sizing_source.clone()),
            );
            push(
                ThermalObjectKind::BalancedVentilation,
                &system.id,
                Some(&zone.id),
                &system.id,
                "/balanced_ventilation",
                serde_json::to_value(&system.balanced_ventilation)?,
                &[],
                Some(system.sizing_source.clone()),
            );
        }
    }
    for shade in &model.detached_shading {
        push(
            ThermalObjectKind::DetachedShade,
            &shade.id,
            None,
            &shade.id,
            "",
            serde_json::to_value(shade)?,
            &[],
            None,
        );
    }
    push(
        ThermalObjectKind::Calendar,
        "calendar",
        None,
        "study:calendar",
        "",
        serde_json::to_value(&snapshot.calendar)?,
        &[],
        None,
    );
    push(
        ThermalObjectKind::Study,
        "study",
        None,
        "study",
        "",
        serde_json::to_value(&snapshot.study)?,
        &["calendar"],
        None,
    );
    Ok(result)
}

fn leaves(value: &Value, path: &str, rows: &mut Vec<(String, Value)>) -> Result<()> {
    if rows.len() >= MAX_PROVENANCE_ROWS {
        return Err(invalid(
            "revision.inputs",
            "Supported-input checklist exceeds 65,536 rows; split the declared study",
        ));
    }
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, value) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                leaves(value, &format!("{path}/{key}"), rows)?;
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (index, value) in items.iter().enumerate() {
                leaves(value, &format!("{path}/{index}"), rows)?;
            }
        }
        _ => rows.push((path.into(), value.clone())),
    }
    Ok(())
}

fn status(assumptions: &[ScopeAssumption]) -> ProvenanceStatus {
    let Some(first) = assumptions.first() else {
        return ProvenanceStatus::Unreviewed;
    };
    if assumptions.iter().any(|item| {
        item.status != first.status || item.value != first.value || item.units != first.units
    }) {
        return ProvenanceStatus::Conflicting;
    }
    match first.status {
        AssumptionStatus::Documented => ProvenanceStatus::Documented,
        AssumptionStatus::Measured => ProvenanceStatus::Measured,
        AssumptionStatus::Assumed => ProvenanceStatus::Assumed,
        AssumptionStatus::Synthetic => ProvenanceStatus::Synthetic,
        AssumptionStatus::Missing => ProvenanceStatus::Missing,
        AssumptionStatus::NotApplicable => ProvenanceStatus::NotApplicable,
    }
}
fn covers(parent: &str, child: &str) -> bool {
    child == parent
        || child
            .strip_prefix(parent)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub fn provenance_checklist(
    designs: &BTreeMap<String, RevisionDesignSnapshot>,
    scope: &ArchitecturalScope,
) -> Result<ProvenanceChecklist> {
    validate_declaration(scope)?;
    let mut rows = vec![];
    for (design_id, design) in designs {
        for object in input_objects(design)? {
            let mut fields = vec![];
            leaves(&object.value, "", &mut fields)?;
            for (field_path, applied_value) in fields {
                if rows.len() >= MAX_PROVENANCE_ROWS {
                    return Err(invalid(
                        "revision.provenance",
                        "Checklist exceeds 65,536 rows; no rows were silently omitted",
                    ));
                }
                let assumption_field = format!("{}{field_path}", object.assumption_prefix);
                let mut source_declarations = scope
                    .assumptions
                    .iter()
                    .filter(|item| match &item.link {
                        AssumptionLink::ThermalObject {
                            design_id: a,
                            object_id: b,
                        } => {
                            a == design_id
                                && b == &object.assumption_object
                                && covers(&item.field, &assumption_field)
                        }
                        // Study links use this explicit derived convention. Legacy
                        // descriptive study text remains present but is not inferred.
                        AssumptionLink::Study => {
                            item.field
                                == format!("{design_id}/{}{field_path}", object.assumption_object)
                        }
                        AssumptionLink::ReferenceOccurrence { .. } => false,
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                source_declarations.sort_by(|a, b| a.id.cmp(&b.id));
                rows.push(InputProvenanceRow {
                    id: canonical_hash(&(
                        "greenlens.provenance-row.v1",
                        design_id,
                        &object.object,
                        &field_path,
                    ))?,
                    design_id: design_id.clone(),
                    object: object.object.clone(),
                    units: thermal_input_unit(&object.object, &field_path, &design.model),
                    field_path,
                    applied_value,
                    declared_source: object.source.clone(),
                    status: status(&source_declarations),
                    source_declarations,
                });
            }
        }
    }
    rows.sort_by(|a, b| {
        (&a.design_id, &a.object, &a.field_path).cmp(&(&b.design_id, &b.object, &b.field_path))
    });
    let unreviewed_count = rows
        .iter()
        .filter(|r| r.status == ProvenanceStatus::Unreviewed)
        .count();
    let missing_count = rows
        .iter()
        .filter(|r| r.status == ProvenanceStatus::Missing || r.applied_value.is_null())
        .count();
    let conflicting_count = rows
        .iter()
        .filter(|r| r.status == ProvenanceStatus::Conflicting)
        .count();
    let result = ProvenanceChecklist {
        schema_version: PROVENANCE_CHECKLIST_SCHEMA.into(), rows, unreviewed_count, missing_count, conflicting_count,
        documentation_complete: unreviewed_count == 0 && missing_count == 0 && conflicting_count == 0,
        disclosure: "Source declarations and applied analytical values are separate. A citation alone does not establish review, equivalence, calibration or scientific qualification. Undocumented valid inputs may execute as Unassessed; this checklist is not an execution prohibition. Geometry does not supply thermal properties, schedules or HVAC. Weather and carbon evidence require their own exact report bindings.".into(),
    };
    bound(&result, MAX_CARRY_FORWARD_BYTES, "revision.provenance")?;
    Ok(result)
}

fn review_row(
    rows: &mut Vec<CarryForwardRow>,
    kind: RevisionRowKind,
    key: String,
    change: RevisionChangeKind,
    before: Option<Value>,
    after: Option<Value>,
    reason: &str,
) -> Result<()> {
    if rows.len() >= MAX_REVIEW_ROWS {
        return Err(invalid(
            "revision.review",
            "Carry-forward exceeds 65,536 rows; no history was pruned",
        ));
    }
    rows.push(CarryForwardRow {
        id: canonical_hash(&("greenlens.carry-forward-row.v1", kind, &key))?,
        kind,
        key,
        change,
        requires_review: change != RevisionChangeKind::Unchanged,
        before,
        after,
        reason: reason.into(),
    });
    Ok(())
}

fn assumption_value(
    assumption: &ScopeAssumption,
    reference: &ReferenceModel,
    designs: &BTreeMap<String, RevisionDesignSnapshot>,
) -> Result<Option<Value>> {
    match &assumption.link {
        AssumptionLink::Study => Ok(Some(serde_json::to_value(designs)?)),
        AssumptionLink::ReferenceOccurrence { occurrence_id } => Ok(reference
            .occurrences
            .iter()
            .find(|o| &o.id == occurrence_id)
            .map(serde_json::to_value)
            .transpose()?),
        AssumptionLink::ThermalObject {
            design_id,
            object_id,
        } => {
            let Some(design) = designs.get(design_id) else {
                return Ok(None);
            };
            let snapshots = thermal_object_snapshots(&design.model)?;
            let Some(snapshot) = snapshots.get(object_id) else {
                return Ok(None);
            };
            let object = if object_id.starts_with("overhang:") {
                snapshot.get(5).unwrap_or(snapshot)
            } else if snapshot.is_array() {
                snapshot.get(0).unwrap_or(snapshot)
            } else {
                snapshot
            };
            Ok(object.pointer(&assumption.field).cloned())
        }
    }
}

pub fn prepare_carry_forward(
    previous: &ScopeRevision,
    reference: &RevisionReferenceSnapshot,
    designs: &BTreeMap<String, RevisionDesignSnapshot>,
) -> Result<CarryForwardPlan> {
    validate_revision(previous)?;
    let models = validate_revision_context(reference, designs)?;
    let context = revision_context_sha256(reference, designs)?;
    let same_reference = previous.reference.manifest.revision_sha256
        == reference.manifest.revision_sha256
        && previous.reference.manifest.geometry_sha256 == reference.manifest.geometry_sha256
        && previous.reference.manifest.options == reference.manifest.options;
    let old_geometry = previous
        .designs
        .iter()
        .map(|(id, d)| Ok((id.clone(), geometry_signatures(d)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let new_geometry = designs
        .iter()
        .map(|(id, d)| Ok((id.clone(), geometry_signatures(d)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let geometry_unchanged = |design: &str, object: &str| {
        let old = old_geometry
            .get(design)
            .and_then(|objects| objects.get(object));
        old.is_some()
            && old
                == new_geometry
                    .get(design)
                    .and_then(|objects| objects.get(object))
    };
    let mut scope = previous.scope.clone();
    scope.reference_revision_sha256 = reference.manifest.revision_sha256.clone();
    scope.reference_geometry_sha256 = reference.manifest.geometry_sha256.clone();
    scope.reference_options_sha256 = canonical_hash(&reference.manifest.options)?;
    scope.design_model_sha256 = models
        .iter()
        .filter(|(id, _)| previous.scope.design_model_sha256.contains_key(*id))
        .map(|(id, model)| (id.clone(), model.model_input_sha256.clone()))
        .collect();
    if scope.design_model_sha256.is_empty() {
        return Err(invalid(
            "revision.designs",
            "No previously scoped design remains; create a new explicitly scoped initial revision",
        ));
    }
    let mut rows = vec![];
    let old_ids = previous
        .reference
        .manifest
        .occurrences
        .iter()
        .map(|o| (o.id.clone(), o))
        .collect::<BTreeMap<_, _>>();
    let new_ids = reference
        .manifest
        .occurrences
        .iter()
        .map(|o| (o.id.clone(), o))
        .collect::<BTreeMap<_, _>>();
    for id in old_ids
        .keys()
        .chain(new_ids.keys())
        .collect::<BTreeSet<_>>()
    {
        let before = old_ids.get(id).map(serde_json::to_value).transpose()?;
        let after = new_ids.get(id).map(serde_json::to_value).transpose()?;
        let change = match (&before, &after) {
            (Some(a), Some(b)) if same_reference && a == b => RevisionChangeKind::Unchanged,
            (Some(_), None) => RevisionChangeKind::Deleted,
            (None, Some(_)) => RevisionChangeKind::Added,
            _ => RevisionChangeKind::Changed,
        };
        review_row(&mut rows, RevisionRowKind::Reference, id.clone(), change, before, after,
            "Occurrences are revision-scoped; equal display names never establish source equivalence.")?;
    }
    scope.mappings.clear();
    for mapping in &previous.scope.mappings {
        let exists = new_ids.contains_key(&mapping.occurrence_id);
        let retained = same_reference
            && exists
            && mapping
                .targets
                .iter()
                .all(|t| geometry_unchanged(&t.design_id, &t.object_id));
        let mut proposed = mapping.clone();
        if retained {
            for target in &mut proposed.targets {
                target.model_input_sha256 = models[&target.design_id].model_input_sha256.clone();
            }
        } else {
            proposed.status = MappingStatus::Unresolved;
            proposed.targets.clear();
            proposed.rationale = "Review required: source revision or analytical geometry changed. The previous mapping and rationale remain in the immutable parent revision.".into();
        }
        review_row(
            &mut rows,
            RevisionRowKind::Mapping,
            mapping.occurrence_id.clone(),
            if retained {
                RevisionChangeKind::Unchanged
            } else if !exists {
                RevisionChangeKind::Deleted
            } else {
                RevisionChangeKind::Unresolved
            },
            Some(serde_json::to_value(mapping)?),
            exists
                .then(|| serde_json::to_value(&proposed))
                .transpose()?,
            if retained {
                "Exact source revision and target geometry are unchanged. Proposed model identity is rebound only on explicit revision acceptance."
            } else {
                "Source revision or generated target geometry changed; explicitly reconcile this mapping. Historical targets were not attached to new geometry."
            },
        )?;
        if exists {
            scope.mappings.push(proposed);
        }
    }
    for id in new_ids.keys() {
        if !scope.mappings.iter().any(|m| &m.occurrence_id == id) {
            scope.mappings.push(ScopeMapping { occurrence_id: id.clone(), status: MappingStatus::Unresolved, targets: vec![], rationale: "New or reimported occurrence requires explicit analytical scope review.".into() });
        }
    }
    scope.assumptions.clear();
    for assumption in &previous.scope.assumptions {
        let before = assumption_value(assumption, &previous.reference.manifest, &previous.designs)?;
        let after = assumption_value(assumption, &reference.manifest, designs)?;
        let source_changed = matches!(assumption.link, AssumptionLink::ReferenceOccurrence { .. })
            && !same_reference;
        let changed = before != after || source_changed;
        let mut proposed = assumption.clone();
        if changed {
            proposed.status = AssumptionStatus::Missing;
            // The v1 declaration requires a missing value to be absent. The
            // exact old declaration remains in the immutable parent and the
            // before side of the review, never relabelled as the new input.
            proposed.value = None;
        }
        review_row(&mut rows, RevisionRowKind::Assumption, assumption.id.clone(),
            if after.is_none() { RevisionChangeKind::Deleted } else if changed { RevisionChangeKind::Changed } else { RevisionChangeKind::Unchanged },
            Some(json!({"declaration":assumption,"applied_value":before})),
            after.as_ref().map(|v| json!({"declaration":proposed,"applied_value":v})),
            "Declared source value is preserved separately from the applied analytical value. Changed input requires review; a missing target is not rebound.")?;
        if after.is_some() {
            scope.assumptions.push(proposed);
        }
    }
    scope.reconciliations.clear();
    for row in &previous.scope.reconciliations {
        let exists = designs.contains_key(&row.design_id)
            && row.occurrence_ids.iter().all(|id| new_ids.contains_key(id))
            && row.analytical_object_ids.iter().all(|id| {
                new_geometry
                    .get(&row.design_id)
                    .is_some_and(|m| m.contains_key(id))
            });
        let unchanged = exists
            && same_reference
            && row
                .analytical_object_ids
                .iter()
                .all(|id| geometry_unchanged(&row.design_id, id));
        review_row(&mut rows, RevisionRowKind::Reconciliation, row.id.clone(),
            if !exists { RevisionChangeKind::Deleted } else if unchanged { RevisionChangeKind::Unchanged } else { RevisionChangeKind::Changed },
            Some(serde_json::to_value(row)?), unchanged.then(|| serde_json::to_value(row)).transpose()?,
            "Retain measured reconciliation only when the exact source and analytical geometry remain unchanged; changed measurements require explicit replacement.")?;
        if unchanged {
            scope.reconciliations.push(row.clone());
        }
    }
    scope.report_views.clear();
    for view in &previous.scope.report_views {
        let unchanged = same_reference && geometry_unchanged(&view.design_id, "model:building");
        let mut proposed = view.clone();
        if let Some(model) = models.get(&view.design_id) {
            proposed.model_input_sha256 = model.model_input_sha256.clone();
        }
        review_row(&mut rows, RevisionRowKind::ReportView, view.id.clone(),
            if unchanged { RevisionChangeKind::Unchanged } else { RevisionChangeKind::Changed },
            Some(serde_json::to_value(view)?), unchanged.then(|| serde_json::to_value(&proposed)).transpose()?,
            "Keep an exact frame only for unchanged source and analytical geometry; otherwise explicitly review or create a new view.")?;
        if unchanged {
            scope.report_views.push(proposed);
        }
    }
    if !same_reference
        || previous
            .designs
            .keys()
            .any(|id| !geometry_unchanged(id, "model:building"))
    {
        scope.coverage = ScopeCoverage::Unknown { reason: "Source or analytical geometry changed. Review the coverage denominator and measurements before declaring coverage.".into() };
        scope.whole_building_review = None;
        if scope.scope_kind == ScopeKind::WholeBuilding {
            scope.scope_kind = ScopeKind::PartialBuilding;
        }
        review_row(&mut rows, RevisionRowKind::Scope, "coverage-and-scope".into(), RevisionChangeKind::Changed,
            Some(json!({"kind":previous.scope.scope_kind,"coverage":previous.scope.coverage,"whole_building_review":previous.scope.whole_building_review})),
            Some(json!({"kind":scope.scope_kind,"coverage":scope.coverage,"whole_building_review":scope.whole_building_review})),
            "A changed source or analytical geometry invalidates the previous coverage conclusion. The proposal discloses unknown coverage and requires explicit scope review.")?;
    }
    let before = provenance_checklist(&previous.designs, &previous.scope)?
        .rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let after = provenance_checklist(designs, &scope)?
        .rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    for id in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        let a = before.get(id);
        let b = after.get(id);
        if a.map(|r| (&r.applied_value, &r.declared_source))
            == b.map(|r| (&r.applied_value, &r.declared_source))
        {
            continue;
        }
        review_row(&mut rows, RevisionRowKind::Input, id.clone(),
            if a.is_none() { RevisionChangeKind::Added } else if b.is_none() { RevisionChangeKind::Deleted } else { RevisionChangeKind::Changed },
            a.map(serde_json::to_value).transpose()?, b.map(serde_json::to_value).transpose()?,
            "The accepted analytical value or declared source changed. Review applicability without implying physical validation.")?;
    }
    validate_declaration(&scope)?;
    let mut plan = CarryForwardPlan {
        schema_version: CARRY_FORWARD_SCHEMA.into(),
        parent_revision_sha256: previous.revision_sha256.clone(),
        target_context_sha256: context,
        proposed_scope: scope,
        rows,
        plan_sha256: String::new(),
    };
    bound(&plan, MAX_CARRY_FORWARD_BYTES, "revision.plan")?;
    plan.plan_sha256 = plan.identity()?;
    Ok(plan)
}

fn validate_decisions(decisions: &[ReviewDecision]) -> Result<()> {
    if decisions.len() > MAX_REVIEW_ROWS {
        return Err(invalid("revision.decisions", "Too many review decisions"));
    }
    let mut ids = BTreeSet::new();
    for decision in decisions {
        hash("revision.decision.row_id", &decision.row_id)?;
        text("revision.decision.rationale", &decision.rationale)?;
        if decision.rationale.len() > 2048 || !ids.insert(&decision.row_id) {
            return Err(invalid(
                "revision.decisions",
                "Review decisions require unique rows and rationale within 2048 bytes",
            ));
        }
    }
    Ok(())
}

/// Recompute the preview against exact inputs before accepting one complete
/// candidate. The caller checks cancellation before publishing this return value.
pub fn accept_carry_forward(
    previous: &ScopeRevision,
    reference: RevisionReferenceSnapshot,
    designs: BTreeMap<String, RevisionDesignSnapshot>,
    expected_plan_sha256: &str,
    scope: ArchitecturalScope,
    mut decisions: Vec<ReviewDecision>,
) -> Result<ScopeRevision> {
    let plan = prepare_carry_forward(previous, &reference, &designs)?;
    if plan.plan_sha256 != expected_plan_sha256 {
        return Err(invalid(
            "revision.plan_sha256",
            "Inputs or source changed after preview; review a fresh carry-forward plan",
        ));
    }
    validate_decisions(&decisions)?;
    let ids = decisions
        .iter()
        .map(|d| d.row_id.as_str())
        .collect::<BTreeSet<_>>();
    if plan
        .rows
        .iter()
        .any(|row| row.requires_review && !ids.contains(row.id.as_str()))
        || decisions
            .iter()
            .any(|decision| !plan.rows.iter().any(|row| row.id == decision.row_id))
    {
        return Err(invalid(
            "revision.review",
            "Explicitly review every affected row; unknown review rows are not accepted",
        ));
    }
    for decision in &decisions {
        if decision.resolution != ReviewResolution::Unresolved {
            continue;
        }
        let row = plan
            .rows
            .iter()
            .find(|row| row.id == decision.row_id)
            .ok_or_else(|| invalid("revision.review", "Unknown review row"))?;
        let presented_as_resolved = match row.kind {
            RevisionRowKind::Mapping => scope
                .mappings
                .iter()
                .any(|m| m.occurrence_id == row.key && m.status != MappingStatus::Unresolved),
            RevisionRowKind::Assumption => scope
                .assumptions
                .iter()
                .any(|a| a.id == row.key && a.status != AssumptionStatus::Missing),
            RevisionRowKind::Reconciliation => {
                scope.reconciliations.iter().any(|r| r.id == row.key)
            }
            RevisionRowKind::ReportView => scope.report_views.iter().any(|v| v.id == row.key),
            RevisionRowKind::Scope => {
                !matches!(scope.coverage, ScopeCoverage::Unknown { .. })
                    || scope.scope_kind == ScopeKind::WholeBuilding
            }
            RevisionRowKind::Reference | RevisionRowKind::Input => false,
        };
        if presented_as_resolved {
            return Err(invalid("revision.review", "An unresolved decision cannot be presented as a resolved mapping, measurement, view or source declaration"));
        }
    }
    let models = validate_revision_context(&reference, &designs)?;
    validate_scope(&scope, &reference.manifest, &models)?;
    decisions.sort_by(|a, b| a.row_id.cmp(&b.row_id));
    let provenance = provenance_checklist(&designs, &scope)?;
    let mut revision = ScopeRevision {
        schema_version: SCOPE_REVISION_SCHEMA.into(),
        revision_sha256: String::new(),
        parent_revision_sha256: Some(previous.revision_sha256.clone()),
        reference,
        designs,
        scope,
        carry_forward_plan_sha256: Some(plan.plan_sha256),
        review_decisions: decisions,
        provenance,
    };
    bound(&revision, MAX_REVISION_BYTES, "revision")?;
    revision.revision_sha256 = revision.identity()?;
    validate_revision(&revision)?;
    Ok(revision)
}

/// A deserialized parent/child chain must prove its review binding as well as
/// each envelope digest. Re-sealing edited decisions does not bypass review.
pub fn validate_revision_parent(previous: &ScopeRevision, revision: &ScopeRevision) -> Result<()> {
    validate_revision(revision)?;
    if revision.parent_revision_sha256.as_deref() != Some(&previous.revision_sha256) {
        return Err(invalid(
            "revision.parent",
            "Retained parent identity differs",
        ));
    }
    let accepted = accept_carry_forward(
        previous,
        revision.reference.clone(),
        revision.designs.clone(),
        revision
            .carry_forward_plan_sha256
            .as_deref()
            .ok_or_else(|| invalid("revision.plan", "Missing parent review plan"))?,
        revision.scope.clone(),
        revision.review_decisions.clone(),
    )?;
    if accepted != *revision {
        return Err(invalid(
            "revision",
            "Retained acceptance differs from recomputed review",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::{admit_glb, ReferenceImportOptions};

    fn reference(options: ReferenceImportOptions) -> RevisionReferenceSnapshot {
        // Original two-instance triangle fixture. No building equivalence or
        // engine acceptance is claimed by this metadata/transaction test.
        let mut bin = vec![];
        for value in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            bin.extend(value.to_le_bytes());
        }
        let doc = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],
            "bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
            "nodes":[{"mesh":0,"name":"Repeated source name"},{"mesh":0,"name":"Repeated source name","translation":[2,0,0]}],
            "scenes":[{"nodes":[0,1]}],"scene":0});
        let mut data = serde_json::to_vec(&doc).unwrap();
        while !data.len().is_multiple_of(4) {
            data.push(b' ');
        }
        let mut bytes = vec![];
        bytes.extend(0x4654_6c67u32.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(((28 + data.len() + bin.len()) as u32).to_le_bytes());
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(0x4e4f_534au32.to_le_bytes());
        bytes.extend(data);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e_4942u32.to_le_bytes());
        bytes.extend(bin);
        let manifest = admit_glb(&bytes, &options, |_| true).unwrap().manifest;
        RevisionReferenceSnapshot {
            asset_id: "original-synthetic-reference".into(),
            manifest_sha256: crate::sha256(&serde_json::to_vec(&manifest).unwrap()),
            manifest,
        }
    }

    fn fixture() -> ScopeRevision {
        let reference = reference(ReferenceImportOptions::default());
        let calendar = synthetic_energy_calendar();
        let designs = BTreeMap::from([
            (
                "A".into(),
                RevisionDesignSnapshot {
                    model: synthetic_four_zone_model(false),
                    calendar: calendar.clone(),
                    study: None,
                },
            ),
            (
                "B".into(),
                RevisionDesignSnapshot {
                    model: synthetic_four_zone_model(true),
                    calendar,
                    study: None,
                },
            ),
        ]);
        let models = validate_revision_context(&reference, &designs).unwrap();
        let model = &models["B"];
        let zone = &model.model.zones[0];
        let cop = zone.cooling_system.as_ref().unwrap().gross_rated_cop;
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "Original synthetic scope".into(),
            purpose: "Test revision contracts without claiming reference/building equivalence"
                .into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: reference.manifest.revision_sha256.clone(),
            reference_geometry_sha256: reference.manifest.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&reference.manifest.options).unwrap(),
            design_model_sha256: models
                .iter()
                .map(|(id, model)| (id.clone(), model.model_input_sha256.clone()))
                .collect(),
            mappings: vec![
                ScopeMapping {
                    occurrence_id: reference.manifest.occurrences[0].id.clone(),
                    status: MappingStatus::Mapped,
                    targets: vec![AnalyticalTarget {
                        design_id: "B".into(),
                        object_id: zone.id.clone(),
                        kind: AnalyticalObjectKind::Zone,
                        model_input_sha256: model.model_input_sha256.clone(),
                    }],
                    rationale: "Synthetic test mapping".into(),
                },
                ScopeMapping {
                    occurrence_id: reference.manifest.occurrences[1].id.clone(),
                    status: MappingStatus::Mapped,
                    targets: vec![AnalyticalTarget {
                        design_id: "B".into(),
                        object_id: zone.windows[0].id.clone(),
                        kind: AnalyticalObjectKind::Window,
                        model_input_sha256: model.model_input_sha256.clone(),
                    }],
                    rationale: "Synthetic test mapping".into(),
                },
            ],
            included_scope: vec!["Selected synthetic zone".into()],
            excluded_scope: vec!["No whole-building source equivalence".into()],
            boundary_consequences: "Accepted analytical boundary assumptions remain visible".into(),
            assumptions: vec![ScopeAssumption {
                id: "declared-cop".into(),
                link: AssumptionLink::ThermalObject {
                    design_id: "B".into(),
                    object_id: zone.cooling_system.as_ref().unwrap().id.clone(),
                },
                field: "/gross_rated_cop".into(),
                status: AssumptionStatus::Synthetic,
                value: Some(cop.to_string()),
                units: "1".into(),
                source: "Original synthetic fixture".into(),
                rationale: "No calibrated product claim".into(),
                affected_scope: zone.id.clone(),
            }],
            reconciliations: vec![ScopeReconciliation {
                id: "selected-area".into(),
                design_id: "B".into(),
                occurrence_ids: vec![reference.manifest.occurrences[0].id.clone()],
                analytical_object_ids: vec![zone.id.clone()],
                measure: ReconciliationMeasure::FloorArea,
                source_measurement: None,
                method: "Source area unknown; analytical rectangle retained".into(),
            }],
            coverage: ScopeCoverage::Unknown {
                reason: "No measured source denominator".into(),
            },
            whole_building_review: None,
            report_views: vec![ReferenceReportView {
                schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
                id: "plan".into(),
                title: "Synthetic retained plan".into(),
                design_id: "B".into(),
                model_input_sha256: model.model_input_sha256.clone(),
                reference_revision_sha256: reference.manifest.revision_sha256.clone(),
                reference_geometry_sha256: reference.manifest.geometry_sha256.clone(),
                eye_m: [8., 3., 20.],
                target_m: [8., 3., 0.],
                up: [0., 1., 0.],
                vertical_span_m: 20.,
                aspect_ratio: 1.5,
                section: None,
            }],
        };
        initial_revision(scope, reference, designs).unwrap()
    }
    fn decisions(plan: &CarryForwardPlan) -> Vec<ReviewDecision> {
        plan.rows
            .iter()
            .filter(|row| row.requires_review)
            .map(|row| ReviewDecision {
                row_id: row.id.clone(),
                resolution: ReviewResolution::Reviewed,
                rationale: "Explicit synthetic test review; no physical validation claim".into(),
            })
            .collect()
    }

    #[test]
    fn empty_register_is_incomplete_and_applied_values_are_separate() {
        let revision = fixture();
        assert!(revision.provenance.unreviewed_count > 0);
        assert!(!revision.provenance.documentation_complete);
        let cop = revision
            .provenance
            .rows
            .iter()
            .find(|row| row.design_id == "B" && row.field_path == "/gross_rated_cop")
            .unwrap();
        assert_eq!(cop.status, ProvenanceStatus::Synthetic);
        assert_eq!(cop.units.as_deref(), Some("1"));
        assert!(cop.applied_value.is_number());
        assert!(cop.source_declarations[0].value.is_some());
        let mut scope = revision.scope.clone();
        scope.assumptions.clear();
        let checklist = provenance_checklist(&revision.designs, &scope).unwrap();
        assert!(!checklist.documentation_complete);
        assert_eq!(checklist.unreviewed_count, checklist.rows.len());
        assert!(checklist
            .rows
            .iter()
            .any(|row| row.declared_source.is_some()));
        assert!(checklist
            .rows
            .iter()
            .any(|row| row.object.kind == ThermalObjectKind::Calendar));
        assert!(checklist
            .rows
            .iter()
            .any(|row| row.object.kind == ThermalObjectKind::Schedule
                && row.field_path == "/weekday/values/0"));
    }

    #[test]
    fn cop_retains_geometry_mapping_but_reviews_actual_changed_input() {
        let previous = fixture();
        let old_bytes = serde_json::to_vec(&previous).unwrap();
        let mut designs = previous.designs.clone();
        designs.get_mut("B").unwrap().model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.5;
        assert_eq!(
            geometry_signatures(&previous.designs["B"]).unwrap(),
            geometry_signatures(&designs["B"]).unwrap()
        );
        let plan = prepare_carry_forward(&previous, &previous.reference, &designs).unwrap();
        assert!(plan
            .rows
            .iter()
            .filter(|r| r.kind == RevisionRowKind::Mapping)
            .all(|r| r.change == RevisionChangeKind::Unchanged));
        assert!(plan
            .rows
            .iter()
            .any(|r| r.kind == RevisionRowKind::Input && r.requires_review));
        assert_eq!(
            plan.proposed_scope.mappings[0].status,
            MappingStatus::Mapped
        );
        assert_eq!(
            plan.proposed_scope.reconciliations,
            previous.scope.reconciliations
        );
        assert_eq!(
            plan.proposed_scope.report_views[0].eye_m,
            previous.scope.report_views[0].eye_m
        );
        assert_ne!(
            plan.proposed_scope.report_views[0].model_input_sha256,
            previous.scope.report_views[0].model_input_sha256
        );
        assert_eq!(
            plan.proposed_scope.assumptions[0].status,
            AssumptionStatus::Missing
        );
        assert_eq!(plan.proposed_scope.assumptions[0].value, None);
        assert!(plan
            .rows
            .iter()
            .any(|row| row.kind == RevisionRowKind::Assumption
                && row
                    .before
                    .as_ref()
                    .and_then(|value| value.pointer("/declaration/value"))
                    == Some(&serde_json::to_value(&previous.scope.assumptions[0].value).unwrap())));
        assert!(accept_carry_forward(
            &previous,
            previous.reference.clone(),
            designs.clone(),
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            vec![]
        )
        .is_err());
        let next = accept_carry_forward(
            &previous,
            previous.reference.clone(),
            designs,
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            decisions(&plan),
        )
        .unwrap();
        validate_revision_parent(&previous, &next).unwrap();
        assert_ne!(next.revision_sha256, previous.revision_sha256);
        assert_eq!(old_bytes, serde_json::to_vec(&previous).unwrap());
        assert!(!next.provenance.documentation_complete);
    }

    #[test]
    fn geometry_and_deleted_targets_require_explicit_reconciliation() {
        let previous = fixture();
        let mut designs = previous.designs.clone();
        designs.get_mut("B").unwrap().model.zones[0].windows[0].width_m *= 0.9;
        let plan = prepare_carry_forward(&previous, &previous.reference, &designs).unwrap();
        assert!(plan
            .proposed_scope
            .mappings
            .iter()
            .all(|m| m.status == MappingStatus::Unresolved && m.targets.is_empty()));
        assert!(plan.proposed_scope.reconciliations.is_empty());
        assert!(plan.proposed_scope.report_views.is_empty());
        assert!(plan
            .rows
            .iter()
            .any(|r| r.kind == RevisionRowKind::Scope && r.requires_review));
        designs.get_mut("B").unwrap().model.zones[0]
            .windows
            .remove(0);
        let deleted = prepare_carry_forward(&previous, &previous.reference, &designs).unwrap();
        assert!(deleted
            .rows
            .iter()
            .any(|r| r.kind == RevisionRowKind::Input && r.change == RevisionChangeKind::Deleted));
        assert!(deleted.proposed_scope.mappings[1].targets.is_empty());
        validate_scope(
            &deleted.proposed_scope,
            &previous.reference.manifest,
            &validate_revision_context(&previous.reference, &designs).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn reimport_equal_names_never_rebinds_revision_scoped_occurrences() {
        let previous = fixture();
        let changed = reference(ReferenceImportOptions {
            translation_m: [1., 0., 0.],
            ..Default::default()
        });
        assert_eq!(
            previous.reference.manifest.occurrences[0].label,
            changed.manifest.occurrences[0].label
        );
        let plan = prepare_carry_forward(&previous, &changed, &previous.designs).unwrap();
        assert!(plan
            .proposed_scope
            .mappings
            .iter()
            .all(|m| m.status == MappingStatus::Unresolved && m.targets.is_empty()));
        assert!(plan.proposed_scope.mappings.iter().all(|m| !previous
            .scope
            .mappings
            .iter()
            .any(|old| old.occurrence_id == m.occurrence_id)));
        assert_eq!(
            plan.rows
                .iter()
                .filter(|r| r.kind == RevisionRowKind::Reference
                    && r.change == RevisionChangeKind::Deleted)
                .count(),
            2
        );
        assert_eq!(
            plan.rows
                .iter()
                .filter(|r| r.kind == RevisionRowKind::Reference
                    && r.change == RevisionChangeKind::Added)
                .count(),
            2
        );
        assert!(plan.proposed_scope.report_views.is_empty());
    }

    #[test]
    fn stale_preview_duplicate_decisions_and_resealed_claims_fail() {
        let previous = fixture();
        let mut designs = previous.designs.clone();
        designs.get_mut("B").unwrap().model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.5;
        let plan = prepare_carry_forward(&previous, &previous.reference, &designs).unwrap();
        let mut duplicated = decisions(&plan);
        duplicated.push(duplicated[0].clone());
        assert!(accept_carry_forward(
            &previous,
            previous.reference.clone(),
            designs.clone(),
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            duplicated
        )
        .is_err());
        let mut changed = designs.clone();
        changed.get_mut("B").unwrap().model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.1;
        assert!(accept_carry_forward(
            &previous,
            previous.reference.clone(),
            changed,
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            decisions(&plan)
        )
        .is_err());
        let mut next = accept_carry_forward(
            &previous,
            previous.reference.clone(),
            designs,
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            decisions(&plan),
        )
        .unwrap();
        next.provenance.documentation_complete = true;
        next.revision_sha256 = next.identity().unwrap();
        assert!(validate_revision(&next).is_err());
        assert!(bound(&"x".repeat(100), 20, "fixture").is_err());
        assert!(validate_decisions(&[ReviewDecision {
            row_id: "0".repeat(64),
            resolution: ReviewResolution::Reviewed,
            rationale: "x".repeat(2049)
        }])
        .is_err());
    }

    #[test]
    fn legacy_scope_canonical_bytes_and_model_identities_are_not_rewritten() {
        let original = fixture();
        let bytes = serde_json::to_vec(&original.scope).unwrap();
        let decoded: ArchitecturalScope = serde_json::from_slice(&bytes).unwrap();
        let scope_hash = canonical_hash(&(ARCHITECTURAL_SCOPE_SCHEMA, &decoded)).unwrap();
        let accepted = initial_revision(
            decoded,
            original.reference.clone(),
            original.designs.clone(),
        )
        .unwrap();
        assert_eq!(serde_json::to_vec(&accepted.scope).unwrap(), bytes);
        assert_eq!(
            canonical_hash(&(ARCHITECTURAL_SCOPE_SCHEMA, &accepted.scope)).unwrap(),
            scope_hash
        );
        assert_eq!(
            accepted.scope.design_model_sha256,
            original.scope.design_model_sha256
        );
        let roundtrip: ScopeRevision =
            serde_json::from_slice(&serde_json::to_vec(&accepted).unwrap()).unwrap();
        validate_revision(&roundtrip).unwrap();
    }

    // JSON.parse/stringify emits integral JavaScript numbers without a decimal
    // suffix, including numbers originally serialized from typed f64 fields.
    fn javascript_integral_numbers(value: &mut Value) -> usize {
        match value {
            Value::Array(items) => items.iter_mut().map(javascript_integral_numbers).sum(),
            Value::Object(fields) => fields.values_mut().map(javascript_integral_numbers).sum(),
            Value::Number(number) if number.is_f64() => {
                let n = number.as_f64().unwrap();
                if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
                    *value = json!(n as i64);
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    #[test]
    fn javascript_scope_roundtrip_preserves_exact_revision_bytes_and_identity() {
        let original = fixture();
        let bytes = serde_json::to_vec(&original).unwrap();
        let canonical = crate::canonical_json(&original).unwrap();
        let mut wire = serde_json::to_value(&original).unwrap();
        assert!(javascript_integral_numbers(&mut wire) > 1000);
        assert_ne!(crate::canonical_json(&wire).unwrap(), canonical);
        let restored: ScopeRevision =
            serde_json::from_slice(&serde_json::to_vec(&wire).unwrap()).unwrap();
        validate_revision(&restored).unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.identity().unwrap(), original.revision_sha256);
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
        assert_eq!(crate::canonical_json(&restored).unwrap(), canonical);

        // A second ordinary file deserialize has the same historical identity.
        let from_disk: ScopeRevision = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(from_disk, restored);
    }

    #[test]
    fn javascript_restoration_does_not_accept_changed_provenance() {
        let original = fixture();
        let mut wire = serde_json::to_value(&original).unwrap();
        javascript_integral_numbers(&mut wire);
        let numeric = original
            .provenance
            .rows
            .iter()
            .position(|row| row.applied_value.as_f64() == Some(20.0))
            .unwrap();
        let source = original
            .provenance
            .rows
            .iter()
            .position(|row| !row.source_declarations.is_empty())
            .unwrap();
        for case in 0..13 {
            let mut altered = wire.clone();
            match case {
                0 => altered["provenance"]["rows"][numeric]["applied_value"] = json!(21),
                1 => {
                    altered["provenance"]["rows"][numeric]["applied_value"] = json!(20.000000000001)
                }
                2 => altered["provenance"]["documentation_complete"] = json!(true),
                3 => altered["provenance"]["unreviewed_count"] = json!(0),
                4 => altered["provenance"]["unexpected_field"] = json!("must not disappear"),
                5 => altered["provenance"]["rows"][0]["unexpected_field"] = json!(true),
                6 => altered["provenance"]["rows"][0]["id"] = json!("f".repeat(64)),
                7 => altered["provenance"]["rows"][0]["status"] = json!("documented"),
                8 => altered["provenance"]["rows"][0]["units"] = json!("wrong units"),
                9 => {
                    altered["provenance"]["rows"][source]["source_declarations"][0]["source"] =
                        json!("changed source")
                }
                10 => altered["provenance"]["rows"]
                    .as_array_mut()
                    .unwrap()
                    .swap(0, 1),
                11 => {
                    altered["provenance"]["rows"].as_array_mut().unwrap().pop();
                }
                12 => {
                    altered["provenance"]
                        .as_object_mut()
                        .unwrap()
                        .remove("disclosure");
                }
                _ => unreachable!(),
            }
            assert!(
                serde_json::from_value::<ScopeRevision>(altered).is_err(),
                "Changed provenance case {case} was accepted"
            );
        }
        let mut changed_input = wire;
        changed_input["designs"]["A"]["model"]["exterior_lighting_w"] = json!(21);
        assert!(serde_json::from_value::<ScopeRevision>(changed_input).is_err());
        assert_eq!(original.identity().unwrap(), original.revision_sha256);
    }

    #[test]
    fn numeric_restoration_is_scalar_exact_and_safe_integer_bounded() {
        for (required, received, allowed) in [
            (json!(1.0), json!(1), true),
            (json!(-1.0), json!(-1), true),
            (json!(-0.0), json!(0), true),
            (json!(0.0), json!(0), true),
            (
                json!(9_007_199_254_740_991.0),
                json!(9_007_199_254_740_991_i64),
                true,
            ),
            (
                json!(-9_007_199_254_740_991.0),
                json!(-9_007_199_254_740_991_i64),
                true,
            ),
            (
                json!(9_007_199_254_740_992.0),
                json!(9_007_199_254_740_992_i64),
                false,
            ),
            (
                json!(9_007_199_254_740_992.0),
                json!(9_007_199_254_740_993_i64),
                false,
            ),
            (
                json!(-9_007_199_254_740_992.0),
                json!(-9_007_199_254_740_993_i64),
                false,
            ),
            (json!(1.5), json!(1), false),
            (json!(1.0), json!(1.0000000000000002), false),
            (json!(1), json!(1.0), false),
            (json!(1.0), json!("1"), false),
            (json!(1.0), Value::Null, false),
            (json!([1.0]), json!([1]), false),
            (json!({"nested": 1.0}), json!({"nested": 1}), false),
        ] {
            let mut actual = received;
            restore_provenance_number_representation(&mut actual, &required);
            assert_eq!(
                actual == required,
                allowed,
                "Expected {required}, received {actual}"
            );
        }
        // Status/count metadata and source declarations are outside the only
        // permitted restoration location, even when numbers happen to match.
        let expected = json!({"rows": [{"applied_value": 1.0}], "count": 1.0});
        let mut actual = json!({"rows": [{"applied_value": 1}], "count": 1});
        restore_provenance_number_representation(&mut actual, &expected);
        assert_ne!(actual, expected);
        assert!(serde_json::from_str::<ScopeRevision>("{\"provenance\":NaN}").is_err());
    }

    #[test]
    fn scope_wire_bounds_reject_before_derived_checklist_reconstruction() {
        let mut wire = serde_json::to_value(fixture()).unwrap();
        let row = serde_json::to_value(InputProvenanceRow {
            id: String::new(),
            design_id: String::new(),
            object: ThermalObjectRef::model(),
            field_path: String::new(),
            applied_value: Value::Null,
            units: None,
            declared_source: None,
            status: ProvenanceStatus::Unreviewed,
            source_declarations: vec![],
        })
        .unwrap();
        wire["provenance"]["rows"] = Value::Array(vec![row; MAX_PROVENANCE_ROWS + 1]);
        let error = serde_json::from_value::<ScopeRevision>(wire.clone()).unwrap_err();
        assert!(error.to_string().contains("row limit"));
        wire["provenance"]["rows"] = json!([]);
        wire["provenance"]["disclosure"] = json!("x".repeat(MAX_CARRY_FORWARD_BYTES + 1));
        let error = serde_json::from_value::<ScopeRevision>(wire).unwrap_err();
        assert!(error.to_string().contains("byte limit"));
    }

    #[test]
    fn scope_wire_rejects_duplicate_provenance_metadata_keys() {
        let wire = serde_json::to_string(&fixture()).unwrap();
        let duplicate_summary = wire.replacen(
            "\"documentation_complete\":false",
            "\"documentation_complete\":true,\"documentation_complete\":false",
            1,
        );
        assert_ne!(duplicate_summary, wire);
        let error = serde_json::from_str::<ScopeRevision>(&duplicate_summary).unwrap_err();
        assert!(error.to_string().contains("duplicate field"));
        let duplicate_row = wire.replacen(
            "\"status\":\"unreviewed\"",
            "\"status\":\"documented\",\"status\":\"unreviewed\"",
            1,
        );
        assert_ne!(duplicate_row, wire);
        let error = serde_json::from_str::<ScopeRevision>(&duplicate_row).unwrap_err();
        assert!(error.to_string().contains("duplicate field"));
    }
}
