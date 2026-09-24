//! Cube-sphere patch mesh generation: sphere-aligned positions, normals, UVs,
//! skirt ring, and stitch index variants. Pure domain geometry; no ECS.

use super::{face_uv_to_direction, CubeFace, PatchEdge, TerrainPatch};
use crate::domain::math::DVec3;
use crate::domain::services::terrain_source::TerrainSource;

/// Geometry of a terrain patch: sphere-aligned positions, normals, and indices
/// with a downward skirt ring to hide LOD cracks.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchGeometry {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    /// Stable equirectangular UVs for whole-planet imagery.
    pub uvs: Vec<[f32; 2]>,
    /// Tile-local UVs retained for future custom material normal/detail maps.
    pub local_uvs: Vec<[f32; 2]>,
    /// Per-vertex radial height offset from this patch's surface to the coarser
    /// (parent-level) surface, in meters. The renderer morphs vertices along the
    /// normal by this offset so LOD refinement is continuous instead of popping.
    /// Empty when the resolution cannot resolve a 2:1 parent grid.
    pub morph_deltas: Vec<f32>,
    pub indices: Vec<u32>,
}

/// Build the mesh geometry for a patch from the shared terrain source.
/// `resolution` is the number of vertices per side. Boundary vertices get a
/// skirt ring extruded down the normal by `skirt_depth_m`.
pub fn build_patch_geometry(
    patch: &TerrainPatch,
    source: &dyn TerrainSource,
    planet_radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
) -> PatchGeometry {
    build_patch_geometry_with_stitches(
        patch,
        source,
        planet_radius_m,
        resolution,
        skirt_depth_m,
        &[],
    )
}

/// Build a patch with 2:1 edge stitch index variants for every listed edge.
/// A stitched fine edge references only every second boundary sample, which
/// aligns with the corresponding `2^n+1` coarse grid. Skirts remain below the
/// surface as a defensive fallback for raster precision and multi-edge corners.
pub fn build_patch_geometry_with_stitches(
    patch: &TerrainPatch,
    source: &dyn TerrainSource,
    planet_radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
    stitched_edges: &[PatchEdge],
) -> PatchGeometry {
    let parent_level = patch.level.saturating_sub(1);
    build_patch_geometry_with_height_sampler(
        patch,
        planet_radius_m,
        resolution,
        skirt_depth_m,
        stitched_edges,
        |latitude_deg, longitude_deg| {
            source.mesh_height_m(latitude_deg, longitude_deg, patch.level)
        },
        |latitude_deg, longitude_deg| {
            source.mesh_height_m(latitude_deg, longitude_deg, parent_level)
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn build_patch_geometry_with_height_sampler(
    patch: &TerrainPatch,
    planet_radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
    stitched_edges: &[PatchEdge],
    height_at: impl Fn(f64, f64) -> f64,
    coarse_height_at: impl Fn(f64, f64) -> f64,
) -> PatchGeometry {
    // Sample in planet-tangent coordinates, rather than patch UV space, so
    // shared vertices retain an identical normal across LOD and cube faces.
    // A global footprint keeps shared normals identical across cube faces and
    // LOD transitions while filtering sub-cell micro relief out of macro mesh
    // lighting. Fine patches recover small-scale grain through their normal map.
    const NORMAL_SAMPLE_DISTANCE_M: f64 = 250.0;

    let res = resolution.max(2) as usize;
    let (u0, v0, u1, v1) = patch.uv_bounds();

    let mut grid = vec![[0.0f64; 3]; res * res];
    let mut normals = vec![[0.0f64; 3]; res * res];
    let mut uvs = vec![[0.0f32; 2]; res * res];
    let mut local_uvs = vec![[0.0f32; 2]; res * res];
    let mut morph_deltas = vec![0.0f32; res * res];

    // Parent-level surface on the 2:1 parent grid. With an even number of cells
    // the parent's vertices coincide with this patch's even vertices, so the
    // morph target is a plain bilinear interpolation of those samples (CDLOD).
    // A root has no coarser surface to morph toward.
    let morph_supported = patch.level > 0 && res >= 3 && (res - 1).is_multiple_of(2);
    let half = (res - 1) / 2;
    let coarse_stride = half + 1;
    let mut coarse_heights = vec![0.0f64; coarse_stride * coarse_stride];
    if morph_supported {
        for m in 0..=half {
            for k in 0..=half {
                let u = u0 + (u1 - u0) * (2 * k) as f64 / (res - 1) as f64;
                let v = v0 + (v1 - v0) * (2 * m) as f64 / (res - 1) as f64;
                let dir = face_uv_to_direction(patch.face, u, v);
                let (lat, lon) = direction_to_lat_lon(dir);
                coarse_heights[m * coarse_stride + k] = coarse_height_at(lat, lon);
            }
        }
    }
    let surface_point = |direction: DVec3| {
        let direction = direction.normalize();
        let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
        direction * (planet_radius_m + height_at(latitude_deg, longitude_deg))
    };
    let normal_sample_angle = NORMAL_SAMPLE_DISTANCE_M / planet_radius_m;
    for j in 0..res {
        for i in 0..res {
            let u = u0 + (u1 - u0) * i as f64 / (res - 1) as f64;
            let v = v0 + (v1 - v0) * j as f64 / (res - 1) as f64;
            let dir = face_uv_to_direction(patch.face, u, v);
            let (lat, lon) = direction_to_lat_lon(dir);
            let h = height_at(lat, lon);
            let idx = j * res + i;
            let p = dir * (planet_radius_m + h);
            grid[idx] = p.to_array();
            // Global imagery uses geographic coordinates, not a tile-local
            // projection, so every level shares one continuous Earth albedo.
            uvs[idx] = [
                ((lon + 180.0) / 360.0) as f32,
                ((90.0 - lat) / 180.0) as f32,
            ];
            local_uvs[idx] = [i as f32 / (res - 1) as f32, j as f32 / (res - 1) as f32];
            if morph_supported {
                let k = i / 2;
                let m = j / 2;
                let fi = (i % 2) as f64 * 0.5;
                let fj = (j % 2) as f64 * 0.5;
                let k1 = (k + 1).min(half);
                let m1 = (m + 1).min(half);
                let c00 = coarse_heights[m * coarse_stride + k];
                let c10 = coarse_heights[m * coarse_stride + k1];
                let c01 = coarse_heights[m1 * coarse_stride + k];
                let c11 = coarse_heights[m1 * coarse_stride + k1];
                let near_row = c00 + (c10 - c00) * fi;
                let far_row = c01 + (c11 - c01) * fi;
                let coarse_h = near_row + (far_row - near_row) * fj;
                morph_deltas[idx] = (coarse_h - h) as f32;
            }

            // Sample normals immediately after the vertex position. Eroded
            // terrain tiles are then reused while still resident instead of
            // being regenerated in a later full-mesh normal pass.
            let radial = p.normalize();
            let reference_axis = if radial.y.abs() < 0.9 {
                DVec3::Y
            } else {
                DVec3::X
            };
            let east = reference_axis.cross(radial).normalize();
            let north = radial.cross(east).normalize();
            let east_plus = surface_point((radial + east * normal_sample_angle).normalize());
            let east_minus = surface_point((radial - east * normal_sample_angle).normalize());
            let north_plus = surface_point((radial + north * normal_sample_angle).normalize());
            let north_minus = surface_point((radial - north * normal_sample_angle).normalize());
            let n = (east_plus - east_minus)
                .cross(north_plus - north_minus)
                .normalize_or_zero();
            normals[idx] = if n.dot(p) < 0.0 { -n } else { n }.to_array();
        }
    }

    // Skirt ring: for every boundary vertex append a copy extruded down the
    // normal. The ring hides cracks between patches of different LOD.
    let mut positions = grid.clone();
    let mut all_normals = normals.clone();
    let mut skirt_index = vec![None; res * res];
    let on_boundary = |i: usize, j: usize| i == 0 || i == res - 1 || j == 0 || j == res - 1;
    for j in 0..res {
        for i in 0..res {
            if on_boundary(i, j) {
                let idx = j * res + i;
                let p = DVec3::from_array(grid[idx]);
                let n = DVec3::from_array(normals[idx]);
                positions.push((p - n * skirt_depth_m).to_array());
                all_normals.push(normals[idx]);
                uvs.push(uvs[idx]);
                local_uvs.push(local_uvs[idx]);
                morph_deltas.push(morph_deltas[idx]);
                skirt_index[idx] = Some(positions.len() as u32 - 1);
            }
        }
    }

    // Grid triangles (the original res×res vertices are indices 0..res*res).
    // The UV axes are right-handed about the outward normal on some cube faces
    // (NegX, PosY, NegZ) and left-handed on the others (PosX, NegY, PosZ).
    // Emit indices so the front face always points outward (CCW viewed from outside).
    let reversed = matches!(patch.face, CubeFace::PosX | CubeFace::NegY | CubeFace::PosZ);

    let mut indices = Vec::with_capacity((res - 1) * (res - 1) * 6 + res * 4 * 6);
    for j in 0..res - 1 {
        for i in 0..res - 1 {
            let tl = stitched_grid_index(i, j, res, stitched_edges);
            let tr = stitched_grid_index(i + 1, j, res, stitched_edges);
            let bl = stitched_grid_index(i, j + 1, res, stitched_edges);
            let br = stitched_grid_index(i + 1, j + 1, res, stitched_edges);
            if reversed {
                // Flip the winding so front faces outward on left-handed faces.
                indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            } else {
                indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
            }
        }
    }

    // Skirt quads: one quad per boundary segment (grid vertex → skirt vertex).
    // Opposing edges need opposite winding. Select it from the geometric
    // outward direction instead of applying one face-wide winding rule, which
    // back-face culled two of the four skirt walls.
    let patch_center = face_uv_to_direction(patch.face, (u0 + u1) * 0.5, (v0 + v1) * 0.5);
    let push_skirt_quad = |indices: &mut Vec<u32>, a: u32, b: u32, c: u32, d: u32| {
        let a_position = DVec3::from_array(positions[a as usize]);
        let b_position = DVec3::from_array(positions[b as usize]);
        let c_position = DVec3::from_array(positions[c as usize]);
        let edge_midpoint = (a_position + b_position).normalize();
        let outward =
            (edge_midpoint - patch_center * edge_midpoint.dot(patch_center)).normalize_or_zero();
        let triangle_normal = (b_position - a_position)
            .cross(c_position - a_position)
            .normalize_or_zero();
        if triangle_normal.dot(outward) >= 0.0 {
            indices.extend_from_slice(&[a, b, c, c, b, d]);
        } else {
            indices.extend_from_slice(&[a, c, b, c, d, b]);
        }
    };
    let skirt = |i: usize| skirt_index[i].expect("boundary vertex must have a skirt");
    // Bottom edge (j = 0), left→right.
    for i in 0..res - 1 {
        let a = i;
        let b = i + 1;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Top edge (j = res-1), left→right.
    let top = (res - 1) * res;
    for i in 0..res - 1 {
        let a = top + i;
        let b = top + i + 1;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Left edge.
    for j in 0..res - 1 {
        let a = j * res;
        let b = (j + 1) * res;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Right edge.
    for j in 0..res - 1 {
        let a = j * res + res - 1;
        let b = (j + 1) * res + res - 1;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }

    PatchGeometry {
        positions,
        normals: all_normals,
        uvs,
        local_uvs,
        morph_deltas,
        indices,
    }
}

fn stitched_grid_index(i: usize, j: usize, resolution: usize, stitched_edges: &[PatchEdge]) -> u32 {
    let mut i = i;
    let mut j = j;
    if stitched_edges.contains(&PatchEdge::West) && i == 0 && j % 2 == 1 {
        j -= 1;
    }
    if stitched_edges.contains(&PatchEdge::East) && i + 1 == resolution && j % 2 == 1 {
        j -= 1;
    }
    if stitched_edges.contains(&PatchEdge::South) && j == 0 && i % 2 == 1 {
        i -= 1;
    }
    if stitched_edges.contains(&PatchEdge::North) && j + 1 == resolution && i % 2 == 1 {
        i -= 1;
    }
    (j * resolution + i) as u32
}

/// Direction to latitude/longitude in degrees.
pub fn direction_to_lat_lon(dir: DVec3) -> (f64, f64) {
    crate::domain::services::reference_frames::body_fixed_to_terrain_lat_lon(dir)
}
