use super::*;
pub(crate) type Matrix = [f64; 16];
#[cfg(test)]
pub(crate) fn identity() -> Matrix {
    [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ]
}
pub(crate) fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| {
        let row = i % 4;
        let col = i / 4;
        (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum()
    })
}
pub(crate) fn transform(m: Matrix, v: Vec3) -> ReferenceResult<Vec3> {
    let p = std::array::from_fn(|i| m[i] * v[0] + m[4 + i] * v[1] + m[8 + i] * v[2] + m[12 + i]);
    check(
        p.iter().all(|v: &f64| v.is_finite() && v.abs() <= 1e8),
        "reference.transform",
        "Transformed geometry exceeds finite 100,000,000 metre bound",
    )?;
    Ok(p)
}
pub(crate) fn determinant(m: Matrix) -> f64 {
    m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2])
        + m[8] * (m[1] * m[6] - m[5] * m[2])
}
pub(crate) fn validate_affine(m: Matrix, require_trs: bool) -> ReferenceResult<()> {
    check(
        m.iter().all(|x| x.is_finite()) && m[3] == 0. && m[7] == 0. && m[11] == 0. && m[15] == 1.,
        "reference.matrix",
        "Expected finite affine column-major matrix",
    )?;
    let det = determinant(m);
    check(
        det.is_finite() && det != 0.,
        "reference.matrix",
        "Singular/overflowed transforms are unsupported; export nondegenerate static geometry",
    )?;
    if require_trs {
        for a in 0..3 {
            for b in a + 1..3 {
                let dot: f64 = (0..3).map(|i| m[a * 4 + i] * m[b * 4 + i]).sum();
                let norm_a: f64 = (0..3).map(|i| m[a * 4 + i].powi(2)).sum::<f64>().sqrt();
                let norm_b: f64 = (0..3).map(|i| m[b * 4 + i].powi(2)).sum::<f64>().sqrt();
                check(
                    dot.abs() <= 1e-8 * norm_a * norm_b,
                    "reference.matrix",
                    "glTF local matrix must decompose to TRS without shear",
                )?;
            }
        }
    }
    Ok(())
}
pub(crate) fn trs(t: Vec3, q: [f64; 4], s: Vec3) -> ReferenceResult<Matrix> {
    check(
        t.iter()
            .chain(q.iter())
            .chain(s.iter())
            .all(|v| v.is_finite()),
        "reference.trs",
        "Nonfinite node transform",
    )?;
    check(
        (q.iter().map(|v| v * v).sum::<f64>() - 1.).abs() <= 1e-6,
        "reference.rotation",
        "Rotation quaternion must have unit length",
    )?;
    let [x, y, z, w] = q;
    let m = [
        (1. - 2. * (y * y + z * z)) * s[0],
        2. * (x * y + z * w) * s[0],
        2. * (x * z - y * w) * s[0],
        0.,
        2. * (x * y - z * w) * s[1],
        (1. - 2. * (x * x + z * z)) * s[1],
        2. * (y * z + x * w) * s[1],
        0.,
        2. * (x * z + y * w) * s[2],
        2. * (y * z - x * w) * s[2],
        (1. - 2. * (x * x + y * y)) * s[2],
        0.,
        t[0],
        t[1],
        t[2],
        1.,
    ];
    validate_affine(m, true)?;
    Ok(m)
}
pub(crate) fn root_transform(options: &ReferenceImportOptions) -> Matrix {
    let r = options.clockwise_rotation_deg.to_radians();
    let c = r.cos();
    let s = r.sin();
    let k = options.metres_per_source_unit;
    let mut m = [
        c * k,
        -s * k,
        0.,
        0.,
        0.,
        0.,
        k,
        0.,
        -s * k,
        -c * k,
        0.,
        0.,
        0.,
        0.,
        0.,
        1.,
    ];
    m[12..15].copy_from_slice(&options.translation_m);
    m
}
pub(crate) fn area(v: [Vec3; 3]) -> f64 {
    let a: Vec3 = std::array::from_fn(|i| v[1][i] - v[0][i]);
    let b: Vec3 = std::array::from_fn(|i| v[2][i] - v[0][i]);
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    n.iter().map(|v| v * v).sum::<f64>().sqrt() * 0.5
}
pub(crate) fn near(a: f64, b: f64) -> bool {
    (a - b).abs()
        <= GEOMETRY_ABSOLUTE_TOLERANCE_M + GEOMETRY_RELATIVE_TOLERANCE * a.abs().max(b.abs())
}
pub(crate) fn bounds_near(a: Option<ReferenceBounds>, b: Option<ReferenceBounds>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            (0..3).all(|i| near(a.min_m[i], b.min_m[i]) && near(a.max_m[i], b.max_m[i]))
        }
        _ => false,
    }
}
