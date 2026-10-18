//! Optional exact accepted scope/view and neutral-reference supplement. This
//! module never rebinds an old view, mutates a scope, or runs a simulation.
use crate::{persistence::*, reference, study_revisions, Project};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    architectural_scope::{MappingStatus, ReferenceReportView},
    energy::*,
    reference::{decode_chunk, ReferenceError},
    study_revision::{validate_revision, ReviewResolution, ScopeRevision},
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub use greenlens_core::energy::DesignDrawingSelection as DrawingSelection;
pub const DRAWING_SCHEMA: &str = "greenlens.design-report-drawing.v1";
pub const FIGURE_METHOD: &str = SCOPED_DESIGN_FIGURE_METHOD;
pub const MAX_DRAWING_BYTES: usize = 40 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignReportDrawing {
    pub schema_version: String,
    pub selection: DrawingSelection,
    /// Complete immutable source/provenance/review definitions, not a projection
    /// of whichever scope happens to be active when the report is inspected.
    pub scope_revision: ScopeRevision,
    pub frames: Vec<DesignFigureFrame>,
    pub context: Option<PairedReferenceContext>,
}
fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Design-report drawing cancelled"
    );
    crate::project_open::checkpoint(0)
}
fn defaults(data: &EnergyReportWorkbenchData) -> Result<Vec<DesignFigureFrame>> {
    Ok(default_design_frames(
        &data.base.baseline.model,
        &data.base.baseline.study.calendar,
        &data.base.proposed.model,
        &data.base.proposed.study.calendar,
    )?)
}
fn accepted_frame(view: &ReferenceReportView, index: usize) -> DesignFigureFrame {
    let d: [f64; 3] = std::array::from_fn(|i| view.target_m[i] - view.eye_m[i]);
    let length = d.iter().map(|v| v * v).sum::<f64>().sqrt();
    let unit = d.map(|v| v / length);
    let near = |v: [f64; 3]| (0..3).all(|i| (unit[i] - v[i]).abs() <= 1e-10);
    let kind = if near([0., 0., -1.]) {
        DesignViewKind::Plan
    } else if near([0., 1., 0.]) {
        DesignViewKind::Front
    } else if near([-1., 0., 0.]) {
        DesignViewKind::Right
    } else {
        DesignViewKind::Axonometric
    };
    DesignFigureFrame {
        id: format!("accepted-{}", index + 1),
        title: format!("Accepted {} · shared frame", view.title),
        kind,
        eye_m: view.eye_m,
        target_m: view.target_m,
        up: view.up,
        vertical_span_m: view.vertical_span_m,
        aspect_ratio: view.aspect_ratio,
        section: view.section.clone(),
    }
}
fn bound_frames(
    data: &EnergyReportWorkbenchData,
    revision: &ScopeRevision,
    selection: &DrawingSelection,
) -> Result<Vec<DesignFigureFrame>> {
    selection.validate()?;
    validate_revision(revision)?;
    ensure!(
        selection.scope_revision_sha256 == revision.revision_sha256,
        "Selected scope revision differs from retained drawing"
    );
    for case in [&data.base.baseline, &data.base.proposed] {
        let snapshot = revision
            .designs
            .get(&case.id)
            .context("Selected scope does not contain both report designs")?;
        let actual = validate_thermal_model(&snapshot.model, &snapshot.calendar)?;
        ensure!(actual.model_input_sha256==case.model_input_sha256 && snapshot.calendar==case.study.calendar
            && snapshot.model==case.model,"Selected scope analytical snapshot differs from selected retained run; accept a matching scope or select its historical pair");
        if let Some(study) = &snapshot.study {
            ensure!(
                study == &case.study,
                "Selected scope study/weather/calendar differs from selected retained run"
            );
        }
        ensure!(
            revision.scope.design_model_sha256.get(&case.id) == Some(&case.model_input_sha256),
            "Scope target binding differs from selected model"
        );
    }
    let mut frames = defaults(data)?;
    for (index, id) in selection.accepted_view_ids.iter().enumerate() {
        let view = revision
            .scope
            .report_views
            .iter()
            .find(|view| &view.id == id)
            .context("Selected accepted view is absent from this retained scope")?;
        let case = [&data.base.baseline, &data.base.proposed]
            .into_iter()
            .find(|case| case.id == view.design_id)
            .context("Accepted view belongs to a design outside the selected pair")?;
        ensure!(
            view.model_input_sha256 == case.model_input_sha256,
            "Accepted view belongs to a different analytical snapshot"
        );
        frames.push(accepted_frame(view, index));
    }
    Ok(frames)
}
impl DesignReportDrawing {
    pub fn validate(
        &self,
        data: &EnergyReportWorkbenchData,
        frames: &[DesignFigureFrame],
    ) -> Result<()> {
        ensure!(
            self.schema_version == DRAWING_SCHEMA,
            "Unsupported design-report drawing contract"
        );
        let exact = bound_frames(data, &self.scope_revision, &self.selection)?;
        ensure!(
            self.frames == exact && frames == exact,
            "Drawing must preserve default pair and exact selected accepted camera/section frames"
        );
        ensure!(
            self.selection.include_reference_context == self.context.is_some(),
            "Requested reference context cannot disappear silently"
        );
        if let Some(context) = &self.context {
            context.validate(frames, &self.scope_revision.revision_sha256)?;
            let reference = &self.scope_revision.reference;
            ensure!(
                context.reference_revision_sha256 == reference.manifest.revision_sha256
                    && context.reference_geometry_sha256 == reference.manifest.geometry_sha256
                    && context.manifest_sha256 == reference.manifest_sha256,
                "Reference drawing identities differ from accepted source"
            );
        }
        ensure!(serde_json::to_vec(self)?.len()<=MAX_DRAWING_BYTES,"Retained drawing supplement exceeds 40 MiB; preserve the existing study and choose a separately scoped document");
        Ok(())
    }
    pub fn verify_project(
        &self,
        root: &Path,
        project: &Project,
        data: &EnergyReportWorkbenchData,
        frames: &[DesignFigureFrame],
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.validate(data, frames)?;
        checkpoint(cancel)?;
        let retained = study_revisions::selected(project, &self.scope_revision.revision_sha256)?;
        ensure!(
            retained == &self.scope_revision,
            "Drawing scope snapshot differs from exact retained history"
        );
        // A resealed context array or SVG cannot establish integrity. Re-read
        // actual manifest/chunks and rederive bounded triangles in every frame.
        let expected = prepare(
            root,
            project,
            data,
            defaults(data)?,
            self.selection.clone(),
            cancel,
        )?;
        ensure!(
            expected == *self,
            "Reference drawing differs from the complete admitted source and exact retained views"
        );
        checkpoint(cancel)
    }
    pub fn figures(&self, request: &PairedDesignFigureRequest) -> Result<PairedDesignFigures> {
        ensure!(
            request.frames == self.frames,
            "Paired drawing frame differs from retained supplement"
        );
        Ok(scoped_paired_design_figures(
            request,
            &self.scope_revision.revision_sha256,
            self.context.as_ref(),
        )?)
    }
    pub fn disclosures(&self) -> Vec<String> {
        let revision = &self.scope_revision;
        let p = &revision.provenance;
        let unresolved = revision
            .review_decisions
            .iter()
            .filter(|d| d.resolution == ReviewResolution::Unresolved)
            .count();
        let counts = |status| {
            revision
                .scope
                .mappings
                .iter()
                .filter(|m| m.status == status)
                .count()
        };
        let mut lines=vec![format!("Accepted scope {}: {:?}; complete scope, source declarations, mappings, omissions and review rationales are retained in design-scope-revision.json. Scope acceptance is not whole-building equivalence, physical validation or professional approval.",revision.revision_sha256,revision.scope.scope_kind),format!("Documentation complete: {}. Inputs: {} unreviewed, {} missing, {} conflicting. Review resolution is separate: {unresolved}/{} decisions remain explicitly unresolved. These matters limit interpretation even when a value has a declared source.",p.documentation_complete,p.unreviewed_count,p.missing_count,p.conflicting_count,revision.review_decisions.len()),format!("Mappings: {} mapped, {} context-only, {} excluded, {} unresolved, {} total; complete records are retained. Display context never changes analytical inclusion.",counts(MappingStatus::Mapped),counts(MappingStatus::ContextOnly),counts(MappingStatus::Excluded),counts(MappingStatus::Unresolved),revision.scope.mappings.len())];
        if !self.selection.accepted_view_ids.is_empty() {
            lines.push("Selected accepted camera/section frames are copied exactly and shared by A/B; each original view retains its own design and model binding. Using its frame for the other case does not assert that their geometry is equivalent. No filled cut faces or hidden-surface removal is performed.".into());
        }
        if revision.designs.values().any(|d| d.study.is_none()) {
            lines.push("This scope was accepted without a complete study/weather declaration for at least one design. The report retains exact executed study inputs separately; absent scope documentation is not invented.".into());
        }
        if let Some(context) = &self.context {
            for (frame, layer) in self.frames.iter().zip(&context.layers) {
                lines.push(format!("Reference frame {}: {} of {} visible triangles depicted; {} omitted only from the drawing; all {} admitted triangles examined. {}",frame.id,layer.triangles.len(),layer.visible_triangle_count,layer.omitted_visible_triangle_count,layer.source_triangle_count,layer.unavailable_reason.as_deref().unwrap_or("Reference edges are neutral context, not thermal constructions or analytical coverage.")));
            }
        } else {
            lines.push("Neutral reference context was explicitly not requested; retained scope provenance remains available, but the drawing does not depict source geometry.".into());
        }
        lines
    }
}

pub fn prepare(
    root: &Path,
    project: &Project,
    data: &EnergyReportWorkbenchData,
    default_frames: Vec<DesignFigureFrame>,
    selection: DrawingSelection,
    cancel: &AtomicBool,
) -> Result<DesignReportDrawing> {
    checkpoint(cancel)?;
    ensure!(
        default_frames == defaults(data)?,
        "Supplied default frames differ from the exact retained pair"
    );
    let architecture=project.architecture.as_ref().context("Accept a retained architectural scope before selecting reference context or accepted views")?;
    study_revisions::validate_history(architecture)?;
    let revision = study_revisions::selected(project, &selection.scope_revision_sha256)?.clone();
    let frames = bound_frames(data, &revision, &selection)?;
    let asset = reference::asset(project, Some(&revision.reference.asset_id))?;
    let manifest = reference::verify_asset(root, asset, cancel)?;
    ensure!(
        manifest == revision.reference.manifest
            && asset.manifest_sha256 == revision.reference.manifest_sha256,
        "Retained source differs from accepted scope; active reference substitution is prohibited"
    );
    let context = if selection.include_reference_context {
        // One checked stream feeds every common frame; no repeated per-frame
        // full annual or reference traversal and no sampling before clipping.
        let stream = manifest
            .chunks
            .iter()
            .enumerate()
            .flat_map(|(index, entry)| {
                let result: Result<_> = (|| {
                    checkpoint(cancel)?;
                    let bytes = read_evidence_bytes_checked(
                        &regular_child(root, &format!("{}/{}", asset.path, entry.path))?,
                        entry.byte_length as u64,
                        &mut |n| {
                            checkpoint(cancel)?;
                            crate::project_open::checkpoint(n)
                        },
                    )?;
                    Ok(decode_chunk(&manifest, index, &bytes)?)
                })();
                match result {
                    Ok(triangles) => triangles.into_iter().map(Ok).collect::<Vec<_>>(),
                    Err(error) => vec![Err(ReferenceError::Admission {
                        field: "drawing.reference".into(),
                        message: error.to_string(),
                    })],
                }
            });
        let layers = prepare_reference_layers(&manifest, &revision.scope, &frames, stream, cancel)?;
        let mut context = PairedReferenceContext {
            schema_version: DESIGN_REFERENCE_CONTEXT_SCHEMA.into(),
            scope_revision_sha256: revision.revision_sha256.clone(),
            reference_revision_sha256: manifest.revision_sha256.clone(),
            reference_geometry_sha256: manifest.geometry_sha256.clone(),
            manifest_sha256: asset.manifest_sha256.clone(),
            layers,
            content_sha256: String::new(),
        };
        context.seal()?;
        Some(context)
    } else {
        None
    };
    let drawing = DesignReportDrawing {
        schema_version: DRAWING_SCHEMA.into(),
        selection,
        scope_revision: revision,
        frames,
        context,
    };
    drawing.validate(data, &drawing.frames)?;
    checkpoint(cancel)?;
    Ok(drawing)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use greenlens_core::architectural_scope::*;
    use greenlens_core::canonical_hash;
    use greenlens_core::reference::{admit_glb, ReferenceImportOptions};
    use greenlens_core::study_revision::{
        initial_revision_v3, RevisionDesignSnapshot, RevisionReferenceSnapshot,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    /// Arithmetic-only data remains NotExecuted. It is used for binding checks,
    /// never passed to accepted native/report-evidence publication.
    pub(crate) fn fixture() -> (EnergyReportWorkbenchData, ScopeRevision, DrawingSelection) {
        let mut base = synthetic_energy_report().unwrap();
        // Supported document design IDs, while the artificial run/assessment
        // retains its explicit NotExecuted arithmetic-fixture identity.
        base.baseline.id = "A".into();
        base.proposed.id = "B".into();
        let data = EnergyReportWorkbenchData::new(
            base,
            EnergyReportMetadata::default(),
            vec![EnergyReportProfile::Technical],
            EnergyReportAttachmentSummary {
                included_ids: vec![],
                excluded_count: 0,
                missing_unselected_count: 0,
                inapplicable_unselected_count: 0,
            },
            crate::energy_report_workbench::template_sha256(),
        )
        .unwrap();
        let mut bin = Vec::new();
        for value in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            bin.extend(value.to_le_bytes());
        }
        let doc = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
        let mut j = serde_json::to_vec(&doc).unwrap();
        while !j.len().is_multiple_of(4) {
            j.push(b' ');
        }
        let mut bytes = Vec::new();
        bytes.extend(0x46546c67u32.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(((28 + j.len() + bin.len()) as u32).to_le_bytes());
        bytes.extend((j.len() as u32).to_le_bytes());
        bytes.extend(0x4e4f534au32.to_le_bytes());
        bytes.extend(j);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942u32.to_le_bytes());
        bytes.extend(bin);
        let manifest = admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true)
            .unwrap()
            .manifest;
        let reference = RevisionReferenceSnapshot {
            asset_id: "original-synthetic-drawing-fixture".into(),
            manifest_sha256: greenlens_core::sha256(&serde_json::to_vec(&manifest).unwrap()),
            manifest,
        };
        let case = &data.base.baseline;
        let view = ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "Section 空間".into(),
            title: "Retained original section".into(),
            design_id: case.id.clone(),
            model_input_sha256: case.model_input_sha256.clone(),
            reference_revision_sha256: reference.manifest.revision_sha256.clone(),
            reference_geometry_sha256: reference.manifest.geometry_sha256.clone(),
            eye_m: [1., -10., 1.],
            target_m: [1., 0., 1.],
            up: [0., 0., 1.],
            vertical_span_m: 4.,
            aspect_ratio: 1.5,
            section: Some(ReferenceReportSection {
                axis: ReferenceSectionAxis::Y,
                offset_m: 0.5,
                keep: ReferenceSectionKeep::Above,
            }),
        };
        let designs = [&data.base.baseline, &data.base.proposed]
            .into_iter()
            .map(|c| {
                (
                    c.id.clone(),
                    RevisionDesignSnapshot {
                        model: c.model.clone(),
                        calendar: c.study.calendar.clone(),
                        study: Some(c.study.clone()),
                    },
                )
            })
            .collect();
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "Original synthetic scope".into(),
            purpose: "Binding metadata test only, no real engine evidence".into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: reference.manifest.revision_sha256.clone(),
            reference_geometry_sha256: reference.manifest.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&reference.manifest.options).unwrap(),
            design_model_sha256: BTreeMap::from([
                (
                    data.base.baseline.id.clone(),
                    data.base.baseline.model_input_sha256.clone(),
                ),
                (
                    data.base.proposed.id.clone(),
                    data.base.proposed.model_input_sha256.clone(),
                ),
            ]),
            mappings: vec![],
            included_scope: vec!["Synthetic analytical model".into()],
            excluded_scope: vec!["No physical or source geometry equivalence".into()],
            boundary_consequences: "Existing analytical boundaries unchanged".into(),
            assumptions: vec![],
            reconciliations: vec![],
            coverage: ScopeCoverage::Unknown {
                reason: "No independent source floor area".into(),
            },
            whole_building_review: None,
            report_views: vec![view],
        };
        let revision = initial_revision_v3(scope, reference, designs).unwrap();
        let selection = DrawingSelection {
            schema_version: DESIGN_DRAWING_SELECTION_SCHEMA.into(),
            scope_revision_sha256: revision.revision_sha256.clone(),
            accepted_view_ids: vec!["Section 空間".into()],
            include_reference_context: false,
        };
        (data, revision, selection)
    }
    #[test]
    fn accepted_frame_preserves_exact_section_projection_without_changing_source_view() {
        let view = ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "original-front".into(),
            title: "Retained section".into(),
            design_id: "A".into(),
            model_input_sha256: "a".repeat(64),
            reference_revision_sha256: "b".repeat(64),
            reference_geometry_sha256: "c".repeat(64),
            eye_m: [4., -12., 3.],
            target_m: [4., 2., 3.],
            up: [0., 0., 1.],
            vertical_span_m: 7.25,
            aspect_ratio: 1.73,
            section: Some(ReferenceReportSection {
                axis: ReferenceSectionAxis::Y,
                offset_m: 2.5,
                keep: ReferenceSectionKeep::Above,
            }),
        };
        let original = canonical_hash(&view).unwrap();
        let frame = accepted_frame(&view, 0);
        assert_eq!(frame.kind, DesignViewKind::Front);
        assert_eq!(frame.eye_m, view.eye_m);
        assert_eq!(frame.target_m, view.target_m);
        assert_eq!(frame.up, view.up);
        assert_eq!(frame.vertical_span_m, view.vertical_span_m);
        assert_eq!(frame.aspect_ratio, view.aspect_ratio);
        assert_eq!(frame.section, view.section);
        assert_eq!(canonical_hash(&view).unwrap(), original);
    }
    #[test]
    fn selection_rejects_duplicates_limits_and_unversioned_input() {
        let mut selection = DrawingSelection {
            schema_version: DESIGN_DRAWING_SELECTION_SCHEMA.into(),
            scope_revision_sha256: "a".repeat(64),
            accepted_view_ids: vec!["front".into()],
            include_reference_context: true,
        };
        assert!(selection.validate().is_ok());
        selection.accepted_view_ids.push("front".into());
        assert!(selection.validate().is_err());
        selection.accepted_view_ids = vec!["front".into(), "top".into(), "right".into()];
        assert!(selection.validate().is_err());
        selection.accepted_view_ids.clear();
        selection.schema_version = "future".into();
        assert!(selection.validate().is_err());
    }
    #[test]
    fn exact_historical_scope_and_views_bind_both_models_without_active_replacement() {
        let (data, revision, selection) = fixture();
        let before = canonical_hash(&revision).unwrap();
        let frames = bound_frames(&data, &revision, &selection).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[2].section, revision.scope.report_views[0].section);
        assert_eq!(canonical_hash(&revision).unwrap(), before);
        let drawing = DesignReportDrawing {
            schema_version: DRAWING_SCHEMA.into(),
            selection: selection.clone(),
            scope_revision: revision.clone(),
            frames: frames.clone(),
            context: None,
        };
        assert!(drawing.validate(&data, &frames).is_ok());
        let mut missing = selection.clone();
        missing.accepted_view_ids = vec!["same-name-from-new-reference".into()];
        assert!(bound_frames(&data, &revision, &missing)
            .unwrap_err()
            .to_string()
            .contains("absent"));
        let mut wrong = selection;
        wrong.scope_revision_sha256 = "f".repeat(64);
        assert!(bound_frames(&data, &revision, &wrong).is_err());
        let mut changed = data.clone();
        changed.base.proposed.model.name.push_str(" changed");
        assert!(bound_frames(&changed, &revision, &drawing.selection)
            .unwrap_err()
            .to_string()
            .contains("snapshot differs"));
        let mut calendar = data.clone();
        calendar.base.proposed.study.calendar.start_weekday = Weekday::Tuesday;
        assert!(bound_frames(&calendar, &revision, &drawing.selection).is_err());
        assert_eq!(canonical_hash(&revision).unwrap(), before);
    }
    #[test]
    fn changed_or_resealed_scope_snapshot_and_resized_frame_are_rejected() {
        let (data, mut revision, selection) = fixture();
        let frames = bound_frames(&data, &revision, &selection).unwrap();
        revision.scope.report_views[0].aspect_ratio = 0.75;
        assert!(bound_frames(&data, &revision, &selection).is_err());
        let (data, revision, selection) = fixture();
        let mut drawing = DesignReportDrawing {
            schema_version: DRAWING_SCHEMA.into(),
            selection,
            scope_revision: revision,
            frames: frames.clone(),
            context: None,
        };
        drawing.frames[2].aspect_ratio = 0.75;
        assert!(drawing.validate(&data, &drawing.frames).is_err());
        let disclosure = drawing.disclosures().join("\n");
        assert!(disclosure.contains("Documentation complete: false"));
        assert!(disclosure.contains("not whole-building equivalence"));
        assert!(disclosure.contains("design-scope-revision.json"));
    }
    #[test]
    fn actual_admitted_bytes_historical_reimport_and_resealed_context_fences() {
        let (temp, root, imported, mut scope) = crate::study_revisions::tests::fixture();
        let (data, _, _) = fixture();
        let mut base = data.base.clone();
        base.baseline.id = "A".into();
        base.proposed.id = "B".into();
        let data = EnergyReportWorkbenchData::new(
            base,
            data.metadata.clone(),
            data.profiles.clone(),
            data.attachments.clone(),
            data.template_sha256.clone(),
        )
        .unwrap();
        let asset = reference::asset(&imported.project, None).unwrap();
        let manifest = reference::read_manifest(&root, asset).unwrap();
        scope.report_views.push(ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "Section 空間".into(),
            title: "Original admitted reference close-up".into(),
            design_id: "A".into(),
            model_input_sha256: data.base.baseline.model_input_sha256.clone(),
            reference_revision_sha256: manifest.revision_sha256.clone(),
            reference_geometry_sha256: manifest.geometry_sha256.clone(),
            eye_m: [1., -10., 1.],
            target_m: [1., 0., 1.],
            up: [0., 0., 1.],
            vertical_span_m: 4.,
            aspect_ratio: 1.5,
            section: None,
        });
        let cancel = AtomicBool::new(false);
        let project =
            study_revisions::initialize_scope(&root, &imported.project, scope, &cancel).unwrap();
        let saved = crate::save_project(&root, &project, Some(&imported.revision)).unwrap();
        let revision = study_revisions::revisions(&saved.project).last().unwrap();
        let selection = DrawingSelection {
            schema_version: DESIGN_DRAWING_SELECTION_SCHEMA.into(),
            scope_revision_sha256: revision.revision_sha256.clone(),
            accepted_view_ids: vec!["Section 空間".into()],
            include_reference_context: true,
        };
        let drawing = prepare(
            &root,
            &saved.project,
            &data,
            defaults(&data).unwrap(),
            selection,
            &cancel,
        )
        .unwrap();
        drawing
            .verify_project(&root, &saved.project, &data, &drawing.frames, &cancel)
            .unwrap();
        let mut changed = drawing.clone();
        let context = changed.context.as_mut().unwrap();
        let triangle = context
            .layers
            .iter_mut()
            .flat_map(|l| l.triangles.iter_mut())
            .next()
            .expect("Original source must be visible in a retained frame");
        triangle.vertices_m[0][0] += 0.1;
        context.seal().unwrap();
        changed.validate(&data, &changed.frames).unwrap();
        assert!(changed
            .verify_project(&root, &saved.project, &data, &changed.frames, &cancel)
            .unwrap_err()
            .to_string()
            .contains("complete admitted source"));
        let mut missing = saved.project.clone();
        missing
            .architecture
            .as_mut()
            .unwrap()
            .scope_history
            .as_mut()
            .unwrap()
            .revisions
            .clear();
        assert!(drawing
            .verify_project(&root, &missing, &data, &drawing.frames, &cancel)
            .is_err());
        let mut missing = saved.project.clone();
        missing.architecture.as_mut().unwrap().assets.clear();
        assert!(drawing
            .verify_project(&root, &missing, &data, &drawing.frames, &cancel)
            .is_err());
        let options = ReferenceImportOptions {
            translation_m: [4., 0., 0.],
            ..ReferenceImportOptions::default()
        };
        let reimported = reference::import_project(
            &root,
            &saved.project,
            Some(&saved.revision),
            &temp.path().join("original.glb"),
            &options,
            &cancel,
            |_| true,
        )
        .unwrap();
        assert_ne!(
            reimported
                .project
                .architecture
                .as_ref()
                .unwrap()
                .active_asset_id,
            drawing.scope_revision.reference.asset_id
        );
        drawing
            .verify_project(&root, &reimported.project, &data, &drawing.frames, &cancel)
            .unwrap();
        let old_asset = reference::asset(
            &reimported.project,
            Some(&drawing.scope_revision.reference.asset_id),
        )
        .unwrap();
        let chunk = root.join(&old_asset.path).join(&manifest.chunks[0].path);
        let original = std::fs::read(&chunk).unwrap();
        let mut tampered = original.clone();
        *tampered.last_mut().unwrap() ^= 1;
        std::fs::write(&chunk, &tampered).unwrap();
        let seal_path = root.join(&old_asset.path).join("checksums.json");
        let original_seal = std::fs::read(&seal_path).unwrap();
        let mut seal: Checksums = serde_json::from_slice(&original_seal).unwrap();
        seal.sha256.insert(
            manifest.chunks[0].path.clone(),
            greenlens_core::sha256(&tampered),
        );
        std::fs::write(&seal_path, serde_json::to_vec(&seal).unwrap()).unwrap();
        assert!(drawing
            .verify_project(&root, &reimported.project, &data, &drawing.frames, &cancel)
            .is_err());
        std::fs::write(&chunk, &original).unwrap();
        std::fs::write(&seal_path, &original_seal).unwrap();
        drawing
            .verify_project(&root, &reimported.project, &data, &drawing.frames, &cancel)
            .unwrap();
        assert_eq!(std::fs::read(&chunk).unwrap(), original);
        assert!(drawing
            .verify_project(
                &root,
                &reimported.project,
                &data,
                &drawing.frames,
                &AtomicBool::new(true)
            )
            .is_err());
    }
}
