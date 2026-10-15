//! Owned deterministic orthographic line drawings. Imported scripts/assets are
//! never evaluated. Sectioning clips original edges; it does not create filled
//! cut faces or imply a resolved thermal field.
use greenlens_core::{
    architectural_scope::{
        ArchitecturalScope, MappingStatus, ReferenceReportView, ReferenceSectionAxis,
        ReferenceSectionKeep,
    },
    reference::{ReferenceModel, ReferenceTriangle},
};
use std::collections::BTreeMap;

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt();
    a.map(|x| x / n)
}

/// Signed halfspace clipping. Boundary points are retained, with no invented
/// snapping tolerance. Core validation guarantees finite nondegenerate cameras.
fn clip(a: &mut [f64; 3], b: &mut [f64; 3], normal: [f64; 3], offset: f64) -> bool {
    let da = dot(*a, normal) - offset;
    let db = dot(*b, normal) - offset;
    if da < 0. && db < 0. {
        return false;
    }
    if da < 0. || db < 0. {
        let t = da / (da - db);
        let p = std::array::from_fn(|i| a[i] + t * (b[i] - a[i]));
        if da < 0. {
            *a = p;
        } else {
            *b = p;
        }
    }
    true
}

struct Frame {
    right: [f64; 3],
    up: [f64; 3],
    target: [f64; 3],
    width: f64,
    height: f64,
    scale: f64,
    left: f64,
    top: f64,
}
impl Frame {
    fn new(view: &ReferenceReportView) -> Self {
        let forward = unit(sub(view.target_m, view.eye_m));
        let right = unit(cross(forward, view.up));
        let up = unit(cross(right, forward));
        let height = view.vertical_span_m;
        let width = height * view.aspect_ratio;
        let scale = (820. / width).min(460. / height);
        Self {
            right,
            up,
            target: view.target_m,
            width,
            height,
            scale,
            left: 70. + (820. - width * scale) / 2.,
            top: 105. + (460. - height * scale) / 2.,
        }
    }
    fn project(&self, p: [f64; 3]) -> [f64; 2] {
        let d = sub(p, self.target);
        [
            self.left + (dot(d, self.right) + self.width / 2.) * self.scale,
            self.top + (self.height / 2. - dot(d, self.up)) * self.scale,
        ]
    }
    fn clip(&self, a: &mut [f64; 3], b: &mut [f64; 3]) -> bool {
        [
            (self.right, dot(self.target, self.right) - self.width / 2.),
            (
                self.right.map(|v| -v),
                -dot(self.target, self.right) - self.width / 2.,
            ),
            (self.up, dot(self.target, self.up) - self.height / 2.),
            (
                self.up.map(|v| -v),
                -dot(self.target, self.up) - self.height / 2.,
            ),
        ]
        .iter()
        .all(|(n, d)| clip(a, b, *n, *d))
    }
}

/// A visible placeholder retains the requested figure slot; no replacement
/// geometry, axes, scale bar or historical mapping is presented as the view.
pub(super) fn unavailable(view: &ReferenceReportView, reason: &str) -> String {
    let mut lines = vec!["Retained report view unavailable".to_owned(), view.title.clone(), reason.to_owned(),
        format!("Bound reference revision: {}", view.reference_revision_sha256),
        format!("Bound reference geometry: {}", view.reference_geometry_sha256),
        format!("Bound design: {} / {}", view.design_id, view.model_input_sha256),
        "The exact view definition is retained in architecture-report.json. No geometry is depicted.".to_owned()];
    let mut wrapped = Vec::new();
    for line in lines.drain(..) {
        let chars: Vec<char> = line.chars().collect();
        for part in chars.chunks(100) {
            wrapped.push(part.iter().collect::<String>());
        }
    }
    let mut svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"960\" height=\"680\" viewBox=\"0 0 960 680\"><rect width=\"960\" height=\"680\" fill=\"white\"/>".to_owned();
    for (i, line) in wrapped.iter().enumerate() {
        svg += &format!("<text x=\"32\" y=\"{}\" font-family=\"sans-serif\" font-size=\"13\" fill=\"#874c28\">{}</text>", 48 + i * 23, super::escaped(line));
    }
    svg + "</svg>"
}

pub(super) fn svg(
    reference: &ReferenceModel,
    scope: &ArchitecturalScope,
    triangles: &[ReferenceTriangle],
    view: &ReferenceReportView,
) -> (String, usize) {
    if view.reference_revision_sha256 != reference.revision_sha256
        || view.reference_geometry_sha256 != reference.geometry_sha256
        || scope.reference_revision_sha256 != reference.revision_sha256
        || scope.reference_geometry_sha256 != reference.geometry_sha256
    {
        return (unavailable(view, "The exact reference revision/geometry is unavailable. Geometry is suppressed; a different source must never be drawn in this retained frame."), 0);
    }
    let frame = Frame::new(view);
    let (width, height, scale, left, top) = (
        frame.width,
        frame.height,
        frame.scale,
        frame.left,
        frame.top,
    );
    let section = view.section.as_ref().map(|s| {
        let i = match s.axis {
            ReferenceSectionAxis::X => 0,
            ReferenceSectionAxis::Y => 1,
            ReferenceSectionAxis::Z => 2,
        };
        let sign = if s.keep == ReferenceSectionKeep::Above {
            1.
        } else {
            -1.
        };
        let mut normal = [0.; 3];
        normal[i] = sign;
        (normal, s.offset_m * sign)
    });
    let statuses = scope
        .mappings
        .iter()
        .map(|m| (m.occurrence_id.as_str(), m.status))
        .collect::<BTreeMap<_, _>>();
    let full_title = view.title.chars().count() <= 88;
    let title = if full_title {
        view.title.clone()
    } else {
        format!("{}…", view.title.chars().take(85).collect::<String>())
    };
    let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"960\" height=\"680\" viewBox=\"0 0 960 680\"><rect width=\"960\" height=\"680\" fill=\"white\"/><text x=\"32\" y=\"32\" font-family=\"sans-serif\" font-size=\"18\">{}</text><text x=\"32\" y=\"56\" font-family=\"sans-serif\" font-size=\"12\">Retained orthographic reference view; metre coordinates; design {}</text><text x=\"32\" y=\"77\" font-family=\"sans-serif\" font-size=\"12\">Section: {}. All admitted occurrences; local hide/isolate is not a simulation exclusion.</text><rect x=\"{left:.3}\" y=\"{top:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"#c7d0cd\"/>",super::escaped(&title),super::escaped(&view.design_id),super::escaped(&view.section.as_ref().map(|s|format!("{:?} {:?} {:.4} m",s.axis,s.keep,s.offset_m)).unwrap_or_else(||"none".into())),width*scale,height*scale);
    let mut depicted_triangle_count = 0;
    for triangle in triangles {
        let mut depicted = false;
        let occurrence = &reference.occurrences[triangle.occurrence_index as usize];
        let color = match statuses.get(occurrence.id.as_str()) {
            Some(MappingStatus::Mapped) => "#246954",
            Some(MappingStatus::ContextOnly) => "#7a8588",
            Some(MappingStatus::Excluded) => "#b65d54",
            _ => "#a58027",
        };
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            let mut a = triangle.vertices_m[a];
            let mut b = triangle.vertices_m[b];
            if section.is_some_and(|(n, d)| !clip(&mut a, &mut b, n, d)) {
                continue;
            }
            if !frame.clip(&mut a, &mut b) {
                continue;
            }
            let a = frame.project(a);
            let b = frame.project(b);
            let a = format!("{:.3} {:.3}", a[0], a[1]);
            let b = format!("{:.3} {:.3}", b[0], b[1]);
            // A frame-boundary touch or an edge parallel to the viewing axis
            // can collapse to one SVG coordinate. It is not a visible edge.
            if a == b {
                continue;
            }
            out += &format!(
                "<path d=\"M{a}L{b}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"0.65\"/>"
            );
            depicted = true;
        }
        depicted_triangle_count += usize::from(depicted);
    }
    let bar = 10f64.powf((width / 5.).log10().floor());
    out+=&format!("<path d=\"M70 588h{:.3}\" stroke=\"#213834\" stroke-width=\"3\"/><text x=\"70\" y=\"608\" font-family=\"sans-serif\" font-size=\"12\">{bar} m in view plane. Clipped original edges; no filled cut faces.</text><text x=\"32\" y=\"633\" font-family=\"sans-serif\" font-size=\"12\">Green mapped · grey context · red excluded · amber unresolved. {}</text><text x=\"32\" y=\"655\" font-family=\"sans-serif\" font-size=\"11\">Full view definition, title and revision bindings are retained in architecture-report.json.</text></svg>",bar*scale,super::escaped(&reference.options.declared_north_clockwise_deg.map(|v|format!("Declared true north {v}° clockwise")).unwrap_or_else(||"True north unknown".into())));
    (out, depicted_triangle_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::architectural_scope::REFERENCE_REPORT_VIEW_SCHEMA;

    fn view(eye_m: [f64; 3], up: [f64; 3]) -> ReferenceReportView {
        ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "view-1".into(),
            title: "Original orthographic fixture".into(),
            design_id: "A".into(),
            model_input_sha256: "a".repeat(64),
            reference_revision_sha256: "b".repeat(64),
            reference_geometry_sha256: "c".repeat(64),
            eye_m,
            target_m: [0.; 3],
            up,
            vertical_span_m: 10.,
            aspect_ratio: 1.,
            section: None,
        }
    }

    fn assert_close<const N: usize>(actual: [f64; N], expected: [f64; N]) {
        for (a, e) in actual.into_iter().zip(expected) {
            assert!((a - e).abs() <= 1e-9, "{a} differs from {e}");
        }
    }

    #[test]
    fn top_projection_has_x_right_y_up_and_z_depth() {
        let frame = Frame::new(&view([0., 0., 10.], [0., 1., 0.]));
        assert_close(frame.project([0.; 3]), [480., 335.]);
        assert_close(frame.project([1., 0., 0.]), [526., 335.]);
        assert_close(frame.project([0., 1., 0.]), [480., 289.]);
        assert_close(frame.project([0., 0., 3.]), [480., 335.]);
    }

    #[test]
    fn front_projection_has_x_right_z_up_and_y_depth() {
        let frame = Frame::new(&view([0., -10., 0.], [0., 0., 1.]));
        assert_close(frame.project([0.; 3]), [480., 335.]);
        assert_close(frame.project([1., 0., 0.]), [526., 335.]);
        assert_close(frame.project([0., 0., 1.]), [480., 289.]);
        assert_close(frame.project([0., 3., 0.]), [480., 335.]);
    }

    #[test]
    fn frame_clips_both_axes_and_rejects_outside_segments() {
        let frame = Frame::new(&view([0., -10., 0.], [0., 0., 1.]));
        let (mut a, mut b) = ([-10., 0., 0.], [10., 0., 0.]);
        assert!(frame.clip(&mut a, &mut b));
        assert_close(a, [-5., 0., 0.]);
        assert_close(b, [5., 0., 0.]);
        assert_close(frame.project(a), [250., 335.]);
        assert_close(frame.project(b), [710., 335.]);
        let (mut a, mut b) = ([0., 0., -10.], [0., 0., 10.]);
        assert!(frame.clip(&mut a, &mut b));
        assert_close(a, [0., 0., -5.]);
        assert_close(b, [0., 0., 5.]);
        assert_close(frame.project(a), [480., 565.]);
        assert_close(frame.project(b), [480., 105.]);
        let (mut a, mut b) = ([6., 0., -2.], [6., 0., 2.]);
        assert!(!frame.clip(&mut a, &mut b));
    }

    #[test]
    fn section_then_frame_retains_exact_intersections_without_filled_faces() {
        let frame = Frame::new(&view([0., -10., 0.], [0., 0., 1.]));
        let (mut a, mut b) = ([-10., 0., 0.], [10., 0., 4.]);
        assert!(clip(&mut a, &mut b, [0., 0., 1.], 1.));
        assert!(frame.clip(&mut a, &mut b));
        assert_close(a, [-5., 0., 1.]);
        assert_close(b, [5., 0., 3.]);
        assert_close(frame.project(a), [250., 289.]);
        assert_close(frame.project(b), [710., 197.]);
    }

    #[test]
    fn unavailable_view_retains_escaped_title_and_bindings_without_geometry() {
        let mut view = view([0., -10., 0.], [0., 0., 1.]);
        view.title = "Original <frame> & no replacement".into();
        let rendered = unavailable(&view, "Exact source unavailable");
        for text in [
            "Retained report view unavailable",
            "&lt;frame&gt; &amp;",
            &view.reference_revision_sha256,
            &view.reference_geometry_sha256,
            "architecture-report.json",
        ] {
            assert!(rendered.contains(text));
        }
        assert!(!rendered.contains("<path"));
        assert!(!rendered.contains("<polygon"));
        assert!(!rendered.contains("scale bar"));
    }

    #[test]
    fn halfspace_retains_boundary_and_clips_exact_intersection() {
        let mut a = [-1., 2., 3.];
        let mut b = [1., 4., 5.];
        assert!(clip(&mut a, &mut b, [1., 0., 0.], 0.));
        assert_eq!(a, [0., 3., 4.]);
        assert_eq!(b, [1., 4., 5.]);
        assert!(!clip(&mut a, &mut b, [-1., 0., 0.], 1.));
    }
    #[test]
    fn reversed_edge_produces_same_geometric_clip() {
        let a = [-2., 0., 0.];
        let b = [2., 0., 0.];
        let (mut x, mut y) = (a, b);
        let (mut u, mut v) = (b, a);
        assert!(clip(&mut x, &mut y, [1., 0., 0.], 0.));
        assert!(clip(&mut u, &mut v, [1., 0., 0.], 0.));
        assert_eq!((x, y), (v, u));
    }
}
