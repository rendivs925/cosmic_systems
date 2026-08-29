use bevy::asset::RenderAssetUsages;
use bevy::math::*;
use bevy::prelude::*;
use bevy_mesh::Indices;
use bevy_mesh::PrimitiveTopology;
use std::f32::consts::TAU;

use crate::domain::services::physics;

/// Static solar-system orbit geometry uses a fixed high-fidelity tessellation.
/// Adaptive frame quality must not change an ellipse into visible chords.
pub const ORBIT_RIBBON_SEGMENTS: usize = 1024;

pub fn create_uv_sphere_mesh(meshes: &mut ResMut<Assets<Mesh>>, radius: f32) -> Handle<Mesh> {
    #[cfg(target_arch = "wasm32")]
    let (sectors, stacks) = (32, 16);
    #[cfg(not(target_arch = "wasm32"))]
    let (sectors, stacks) = (64, 32);

    let mesh = Sphere::new(radius).mesh().uv(sectors as u32, stacks as u32);
    meshes.add(mesh)
}

/// Create the continuous globe used outside the local terrain streaming window.
/// The resolution keeps the Earth horizon smooth at orbital altitudes while the
/// local cube-sphere terrain supplies close-range surface detail.
pub fn create_flight_globe_mesh(meshes: &mut ResMut<Assets<Mesh>>, radius: f32) -> Handle<Mesh> {
    let (sectors, stacks) = flight_globe_resolution();

    meshes.add(Sphere::new(radius).mesh().uv(sectors, stacks))
}

fn flight_globe_resolution() -> (u32, u32) {
    #[cfg(target_arch = "wasm32")]
    return (128, 64);
    #[cfg(not(target_arch = "wasm32"))]
    (256, 128)
}

pub fn create_orbit_mesh_ellipse(
    meshes: &mut ResMut<Assets<Mesh>>,
    orbit_shape: &physics::OrbitShape,
    orbit_color: Color,
) -> Handle<Mesh> {
    #[cfg(target_arch = "wasm32")]
    const SEGMENTS: usize = 128;
    #[cfg(not(target_arch = "wasm32"))]
    const SEGMENTS: usize = 256;
    let mut positions = Vec::with_capacity(SEGMENTS);
    let mut normals = Vec::with_capacity(SEGMENTS);
    let mut uvs = Vec::with_capacity(SEGMENTS);
    let mut colors = Vec::with_capacity(SEGMENTS);
    let mut indices = Vec::with_capacity(SEGMENTS * 2);
    let linear_color: LinearRgba = orbit_color.into();
    let color = [
        linear_color.red,
        linear_color.green,
        linear_color.blue,
        linear_color.alpha,
    ];

    let e = orbit_shape.eccentricity.clamp(0.0, 0.99);
    let semi_latus = orbit_shape.semi_major_axis_units * (1.0 - e * e);

    for i in 0..SEGMENTS {
        let true_anomaly = (i as f32 / SEGMENTS as f32) * TAU;
        let radius = semi_latus / (1.0 + e * true_anomaly.cos());

        // Position in orbital plane (periapsis at +X)
        let x_orbital = radius * true_anomaly.cos();
        let z_orbital = radius * true_anomaly.sin();

        // Transform to 3D space using same method as planet position calculation
        let pos_3d = physics::transform_orbital_point(
            x_orbital,
            z_orbital,
            orbit_shape.inclination_rad,
            orbit_shape.long_asc_node_rad,
            orbit_shape.arg_periapsis_rad,
        );

        positions.push([pos_3d.x, pos_3d.y, pos_3d.z]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([true_anomaly / TAU, 0.5]);
        colors.push(color);
        indices.push(i as u32);
        indices.push(((i + 1) % SEGMENTS) as u32);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
}

pub fn create_orbit_ribbon_mesh(
    meshes: &mut ResMut<Assets<Mesh>>,
    orbit_shape: &physics::OrbitShape,
    orbit_color: Color,
    thickness: f32,
    segments: usize,
) -> Handle<Mesh> {
    // Uniform eccentric-anomaly samples keep high-eccentricity ellipses smooth
    // near periapsis without changing their shared orbital-element geometry.
    let segments = segments.max(ORBIT_RIBBON_SEGMENTS);
    let vertex_count = segments * 2;
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);
    let mut indices = Vec::with_capacity(segments * 6);

    let e = orbit_shape.eccentricity.clamp(0.0, 0.99);
    let semi_minor = orbit_shape.semi_major_axis_units * (1.0 - e * e).sqrt();

    // Compute all orbit positions first
    let mut orbit_positions: Vec<Vec3> = Vec::with_capacity(segments);
    for i in 0..segments {
        let eccentric_anomaly = (i as f32 / segments as f32) * TAU;
        let x_orbital = orbit_shape.semi_major_axis_units * (eccentric_anomaly.cos() - e);
        let z_orbital = semi_minor * eccentric_anomaly.sin();
        let pos_3d = physics::transform_orbital_point(
            x_orbital,
            z_orbital,
            orbit_shape.inclination_rad,
            orbit_shape.long_asc_node_rad,
            orbit_shape.arg_periapsis_rad,
        );
        orbit_positions.push(pos_3d);
    }

    // Compute orbit plane normal from two non-collinear points on the orbit
    let orbit_normal = orbit_positions[0]
        .cross(orbit_positions[segments / 4])
        .normalize_or_zero();

    // Clamp thickness for consistent elegant appearance
    let thickness = thickness.clamp(1.0, 15.0);

    let linear_color: LinearRgba = orbit_color.into();
    let color_arr = [
        linear_color.red,
        linear_color.green,
        linear_color.blue,
        linear_color.alpha,
    ];

    for i in 0..segments {
        let pos = orbit_positions[i];
        let prev = orbit_positions[(i + segments - 1) % segments];
        let next = orbit_positions[(i + 1) % segments];
        let tangent = (next - prev).normalize_or_zero();
        let normal = tangent.cross(orbit_normal).normalize_or_zero();

        let left = pos - normal * (thickness * 0.5);
        let right = pos + normal * (thickness * 0.5);

        let u = i as f32 / segments as f32;
        positions.push([left.x, left.y, left.z]);
        positions.push([right.x, right.y, right.z]);
        normals.push([0.0, 1.0, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([u, 0.0]);
        uvs.push([u, 1.0]);
        colors.push(color_arr);
        colors.push(color_arr);

        let i0 = (i * 2) as u32;
        let i1 = i0 + 1;
        let i2 = ((i + 1) % segments * 2) as u32;
        let i3 = i2 + 1;
        indices.extend_from_slice(&[i1, i0, i2, i1, i2, i3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
}

/// Build an open ribbon around a sampled path. Unlike orbital elements, a
/// predicted flight path is not necessarily planar or closed.
pub fn create_polyline_ribbon_mesh(points: &[Vec3], color: Color, thickness: f32) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    if points.len() < 2 {
        return mesh;
    }

    let mut positions = Vec::with_capacity(points.len() * 2);
    let mut normals = Vec::with_capacity(points.len() * 2);
    let mut uvs = Vec::with_capacity(points.len() * 2);
    let mut colors = Vec::with_capacity(points.len() * 2);
    let mut indices = Vec::with_capacity((points.len() - 1) * 6);
    let linear_color: LinearRgba = color.into();
    let color = [
        linear_color.red,
        linear_color.green,
        linear_color.blue,
        linear_color.alpha,
    ];
    let half_width = thickness.clamp(0.1, 15.0) * 0.5;

    let first_tangent = (points[1] - points[0]).normalize_or_zero();
    let first_reference = if first_tangent.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let mut side = first_tangent.cross(first_reference).normalize_or_zero();

    for (index, point) in points.iter().copied().enumerate() {
        let previous = points[index.saturating_sub(1)];
        let next = points[(index + 1).min(points.len() - 1)];
        let tangent = (next - previous).normalize_or_zero();
        // Parallel transport avoids the discontinuous width flips caused by
        // switching a global reference axis along a steep flight path.
        let transported_side = (side - tangent * side.dot(tangent)).normalize_or_zero();
        if transported_side != Vec3::ZERO {
            side = transported_side;
        } else {
            let reference_axis = if tangent.y.abs() < 0.9 {
                Vec3::Y
            } else {
                Vec3::X
            };
            side = tangent.cross(reference_axis).normalize_or_zero();
        }
        let left = point - side * half_width;
        let right = point + side * half_width;
        let u = index as f32 / (points.len() - 1) as f32;

        positions.extend_from_slice(&[left.to_array(), right.to_array()]);
        normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 2]);
        uvs.extend_from_slice(&[[u, 0.0], [u, 1.0]]);
        colors.extend_from_slice(&[color; 2]);
    }

    for index in 0..points.len() - 1 {
        let start = (index * 2) as u32;
        indices.extend_from_slice(&[start, start + 2, start + 1, start + 1, start + 2, start + 3]);
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub fn create_placeholder_orbit_mesh(meshes: &mut ResMut<Assets<Mesh>>) -> Handle<Mesh> {
    let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
    let normals = vec![[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0]];
    let colors = vec![[1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]];
    let indices = vec![0u32, 1u32];

    let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
}

pub fn create_eccentricity_marker_mesh(
    meshes: &mut ResMut<Assets<Mesh>>,
    radius: f32,
) -> Handle<Mesh> {
    // Create a simple glowing sphere for eccentricity markers
    // Use UV sphere for better visual quality at small sizes
    create_uv_sphere_mesh(meshes, radius)
}

pub fn create_ring_mesh(
    meshes: &mut ResMut<Assets<Mesh>>,
    inner_radius: f32,
    outer_radius: f32,
) -> Handle<Mesh> {
    #[cfg(target_arch = "wasm32")]
    const SEGMENTS: usize = 128;
    #[cfg(not(target_arch = "wasm32"))]
    const SEGMENTS: usize = 256;
    let mut positions = Vec::with_capacity((SEGMENTS + 1) * 2);
    let mut normals = Vec::with_capacity((SEGMENTS + 1) * 2);
    let mut uvs = Vec::with_capacity((SEGMENTS + 1) * 2);
    let mut indices = Vec::with_capacity(SEGMENTS * 6);

    for i in 0..=SEGMENTS {
        let t = i as f32 / SEGMENTS as f32;
        let angle = t * TAU;
        let (sin_a, cos_a) = angle.sin_cos();

        positions.push([inner_radius * cos_a, 0.0, inner_radius * sin_a]);
        positions.push([outer_radius * cos_a, 0.0, outer_radius * sin_a]);
        normals.push([0.0, 1.0, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.0, t]);
        uvs.push([1.0, t]);
    }

    for i in 0..SEGMENTS {
        let inner0 = (i * 2) as u32;
        let outer0 = inner0 + 1;
        let inner1 = inner0 + 2;
        let outer1 = inner0 + 3;

        indices.extend_from_slice(&[inner0, outer0, outer1, inner0, outer1, inner1]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_globe_has_orbital_horizon_detail() {
        let (sectors, stacks) = flight_globe_resolution();
        assert!(sectors >= 128);
        assert!(stacks >= 64);
    }

    #[test]
    fn orbit_ribbons_never_degrade_below_the_scientific_render_resolution() {
        assert_eq!(ORBIT_RIBBON_SEGMENTS, 1024);
    }
}
