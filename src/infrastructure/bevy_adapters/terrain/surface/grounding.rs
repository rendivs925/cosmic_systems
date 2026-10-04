//! Visual-only grounding on the streamed triangles, never collision authority.

use crate::domain::services::cube_sphere::{face_uv, PatchGeometry, TerrainPatch};
use bevy::math::DVec3;

/// Intersect a body-fixed radial ray with the displayed cell's triangles.
/// Using actual indices also honours collapsed coarse/fine stitch edges.
pub(crate) fn surface_radius(
    patch: &TerrainPatch,
    geometry: &PatchGeometry,
    direction: DVec3,
) -> Option<f64> {
    let resolution = ((geometry.positions.len() + 8) as f64).sqrt() as usize;
    let resolution = resolution.checked_sub(2)?;
    if resolution < 2 {
        return None;
    }
    let (face, u, v) = face_uv(direction);
    if face != patch.face {
        return None;
    }
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let x = ((u - u0) / (u1 - u0)).clamp(0.0, 1.0) * (resolution - 1) as f64;
    let y = ((v - v0) / (v1 - v0)).clamp(0.0, 1.0) * (resolution - 1) as f64;
    let column = (x.floor() as usize).min(resolution - 2);
    let row = (y.floor() as usize).min(resolution - 2);
    let start = (row * (resolution - 1) + column) * 6;
    for indices in geometry.indices.get(start..start + 6)?.as_chunks::<3>().0 {
        let a = DVec3::from_array(*geometry.positions.get(indices[0] as usize)?);
        let b = DVec3::from_array(*geometry.positions.get(indices[1] as usize)?);
        let c = DVec3::from_array(*geometry.positions.get(indices[2] as usize)?);
        let ab = b - a;
        let ac = c - a;
        let normal = ab.cross(ac);
        let denominator = normal.dot(direction);
        if denominator.abs() < f64::EPSILON {
            continue;
        }
        let radius = normal.dot(a) / denominator;
        let relative = direction * radius - a;
        let d00 = ab.dot(ab);
        let d01 = ab.dot(ac);
        let d11 = ac.dot(ac);
        let det = d00 * d11 - d01 * d01;
        if det.abs() < f64::EPSILON {
            continue;
        }
        let s = (d11 * relative.dot(ab) - d01 * relative.dot(ac)) / det;
        let t = (d00 * relative.dot(ac) - d01 * relative.dot(ab)) / det;
        if radius.is_finite() && radius > 0.0 && s >= -1e-6 && t >= -1e-6 && s + t <= 1.0 + 1e-6 {
            return Some(radius);
        }
    }
    None
}
