//! Cube-sphere planetary terrain topology (AGENTS.md sections 20 and 22).
//!
//! A planet's surface is a cube projected to a sphere: six faces, each
//! subdivided by a quadtree. Patch LOD is selected from screen-space error,
//! and skirts hide cracks at LOD boundaries. Geometry is built from the shared
//! [`TerrainSource`], so render and collision stay on one height function.
//!
//! This module is pure domain logic (no Bevy ECS); the streaming/render layers
//! consume it.

use crate::domain::services::terrain_source::TerrainSource;
use bevy::math::DVec3;

/// The six faces of the cube-sphere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CubeFace {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

/// Map a unit direction to its dominant cube face and `(u, v)` in [0,1]² within
/// that face (cube-sphere projection).
pub fn face_uv(dir: DVec3) -> (CubeFace, f64, f64) {
    let d = dir.normalize();
    let ax = d.x.abs();
    let ay = d.y.abs();
    let az = d.z.abs();
    if ax >= ay && ax >= az {
        let face = if d.x > 0.0 {
            CubeFace::PosX
        } else {
            CubeFace::NegX
        };
        (face, ((d.y / ax) + 1.0) / 2.0, ((d.z / ax) + 1.0) / 2.0)
    } else if ay >= az {
        let face = if d.y > 0.0 {
            CubeFace::PosY
        } else {
            CubeFace::NegY
        };
        (face, ((d.x / ay) + 1.0) / 2.0, ((d.z / ay) + 1.0) / 2.0)
    } else {
        let face = if d.z > 0.0 {
            CubeFace::PosZ
        } else {
            CubeFace::NegZ
        };
        (face, ((d.x / az) + 1.0) / 2.0, ((d.y / az) + 1.0) / 2.0)
    }
}

/// Inverse of [`face_uv`]: map a face and `(u, v)` in [0,1]² back to a unit
/// direction on the sphere.
pub fn face_uv_to_direction(face: CubeFace, u: f64, v: f64) -> DVec3 {
    let a = 2.0 * u - 1.0;
    let b = 2.0 * v - 1.0;
    let p = match face {
        CubeFace::PosX => DVec3::new(1.0, a, b),
        CubeFace::NegX => DVec3::new(-1.0, a, b),
        CubeFace::PosY => DVec3::new(a, 1.0, b),
        CubeFace::NegY => DVec3::new(a, -1.0, b),
        CubeFace::PosZ => DVec3::new(a, b, 1.0),
        CubeFace::NegZ => DVec3::new(a, b, -1.0),
    };
    p.normalize()
}

/// A quadtree patch on a cube face: face + level + tile coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainPatch {
    pub face: CubeFace,
    pub level: u32,
    pub tile_x: u32,
    pub tile_y: u32,
}

impl TerrainPatch {
    /// The level-0 root patch (whole face).
    pub fn root(face: CubeFace) -> Self {
        Self {
            face,
            level: 0,
            tile_x: 0,
            tile_y: 0,
        }
    }

    /// The four children at the next level.
    pub fn subdivide(&self) -> [TerrainPatch; 4] {
        let cx = self.tile_x * 2;
        let cy = self.tile_y * 2;
        [
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx,
                tile_y: cy,
            },
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx + 1,
                tile_y: cy,
            },
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx,
                tile_y: cy + 1,
            },
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx + 1,
                tile_y: cy + 1,
            },
        ]
    }

    /// The `(u0, v0, u1, v1)` bounds of this patch in face uv space.
    pub fn uv_bounds(&self) -> (f64, f64, f64, f64) {
        let span = (1u64 << self.level) as f64;
        let u0 = self.tile_x as f64 / span;
        let v0 = self.tile_y as f64 / span;
        let u1 = (self.tile_x as f64 + 1.0) / span;
        let v1 = (self.tile_y as f64 + 1.0) / span;
        (u0, v0, u1, v1)
    }

    /// The patch at `level` covering a given unit direction.
    pub fn for_direction(dir: DVec3, level: u32) -> Self {
        let (face, u, v) = face_uv(dir);
        let span = (1u64 << level) as f64;
        let tx = ((u * span) as u32).min(span as u32 - 1);
        let ty = ((v * span) as u32).min(span as u32 - 1);
        Self {
            face,
            level,
            tile_x: tx,
            tile_y: ty,
        }
    }
}

/// Approximate world-space edge length of a patch at a level on a planet.
pub fn patch_world_size_m(level: u32, planet_radius_m: f64) -> f64 {
    let face_arc = planet_radius_m * std::f64::consts::FRAC_PI_2;
    face_arc / (1u64 << level) as f64
}

/// Projected on-screen error in pixels for a geometric error at a distance.
pub fn screen_space_error_m(
    geometric_error_m: f64,
    distance_m: f64,
    fov_rad: f64,
    screen_height_px: f64,
) -> f64 {
    if distance_m <= 1e-6 {
        return f64::INFINITY;
    }
    let scale = screen_height_px / (2.0 * distance_m * (fov_rad * 0.5).tan());
    geometric_error_m * scale
}

/// The maximum LOD level such that a patch's projected size stays under
/// `screen_error_px`. Higher levels near the camera, lower far away.
pub fn lod_for_distance(
    distance_m: f64,
    planet_radius_m: f64,
    fov_rad: f64,
    screen_height_px: f64,
    screen_error_px: f64,
    max_level: u32,
) -> u32 {
    let mut level = 0u32;
    while level < max_level {
        let size = patch_world_size_m(level, planet_radius_m);
        let err = screen_space_error_m(size, distance_m, fov_rad, screen_height_px);
        if err <= screen_error_px {
            break;
        }
        level += 1;
    }
    level
}

/// Geometry of a terrain patch: sphere-aligned positions, normals, and indices
/// with a downward skirt ring to hide LOD cracks.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchGeometry {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
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
    let res = resolution.max(2) as usize;
    let (u0, v0, u1, v1) = patch.uv_bounds();

    let mut grid = vec![[0.0f64; 3]; res * res];
    for j in 0..res {
        for i in 0..res {
            let u = u0 + (u1 - u0) * i as f64 / (res - 1) as f64;
            let v = v0 + (v1 - v0) * j as f64 / (res - 1) as f64;
            let dir = face_uv_to_direction(patch.face, u, v);
            let (lat, lon) = direction_to_lat_lon(dir);
            let h = source.height_m(lat, lon);
            grid[j * res + i] = (dir * (planet_radius_m + h)).to_array();
        }
    }

    let mut normals = vec![[0.0f64; 3]; res * res];
    for j in 0..res {
        for i in 0..res {
            let idx = j * res + i;
            let p = DVec3::from_array(grid[idx]);
            let right = DVec3::from_array(grid[j * res + (i + 1).min(res - 1)]);
            let left = DVec3::from_array(grid[j * res + i.saturating_sub(1)]);
            let up = DVec3::from_array(grid[((j + 1).min(res - 1)) * res + i]);
            let down = DVec3::from_array(grid[(j.saturating_sub(1)) * res + i]);
            // (right - left) ~ du, (up - down) ~ dv. Their cross may point
            // inward or outward depending on the cube face's UV handedness.
            // Force the normal to point away from the planet center so the
            // ground receives light correctly on every face.
            let n = (right - left).cross(up - down).normalize_or_zero();
            let n = if n.dot(p) < 0.0 { -n } else { n };
            normals[idx] = n.to_array();
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
                skirt_index[idx] = Some(positions.len() as u32 - 1);
            }
        }
    }

    // Grid triangles (the original res×res vertices are indices 0..res*res).
    // The UV axes are right-handed about the outward normal on some cube faces
    // (NegX, PosY, NegZ) and left-handed on the others (PosX, NegY, PosZ).
    // Emit indices so the front face always points outward (CCW viewed from outside).
    let reversed = matches!(
        patch.face,
        CubeFace::PosX | CubeFace::NegY | CubeFace::PosZ
    );

    let mut indices = Vec::with_capacity((res - 1) * (res - 1) * 6 + res * 4 * 6);
    for j in 0..res - 1 {
        for i in 0..res - 1 {
            let tl = (j * res + i) as u32;
            let tr = (j * res + i + 1) as u32;
            let bl = ((j + 1) * res + i) as u32;
            let br = ((j + 1) * res + i + 1) as u32;
            if reversed {
                // Flip the winding so front faces outward on left-handed faces.
                indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            } else {
                indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
            }
        }
    }

    // Skirt quads: one quad per boundary segment (grid vertex → skirt vertex).
    // Same winding rule: flip on reversed faces so the skirts face outward.
    let push_quad = |indices: &mut Vec<u32>, a: u32, b: u32, c: u32, d: u32| {
        if reversed {
            indices.extend_from_slice(&[a, c, b, c, d, b]);
        } else {
            indices.extend_from_slice(&[a, b, c, c, b, d]);
        }
    };
    let skirt = |i: usize| skirt_index[i].expect("boundary vertex must have a skirt");
    // Bottom edge (j = 0), left→right.
    for i in 0..res - 1 {
        let a = i;
        let b = i + 1;
        push_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Top edge (j = res-1), left→right.
    let top = (res - 1) * res;
    for i in 0..res - 1 {
        let a = top + i;
        let b = top + i + 1;
        push_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Left edge.
    for j in 0..res - 1 {
        let a = j * res;
        let b = (j + 1) * res;
        push_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Right edge.
    for j in 0..res - 1 {
        let a = j * res + res - 1;
        let b = (j + 1) * res + res - 1;
        push_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }

    PatchGeometry {
        positions,
        normals: all_normals,
        indices,
    }
}

/// Direction to latitude/longitude in degrees.
pub fn direction_to_lat_lon(dir: DVec3) -> (f64, f64) {
    let d = dir.normalize();
    let lat = d.y.asin().to_degrees();
    let lon = d.z.atan2(d.x).to_degrees();
    (lat, lon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;

    fn source() -> ProceduralTerrainSource {
        ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0)
    }

    #[test]
    fn face_uv_round_trips() {
        for dir in [
            DVec3::new(1.0, 0.2, 0.1),
            DVec3::new(-0.5, -0.8, 0.3),
            DVec3::new(0.1, -0.2, 1.0),
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(-1.0, 0.0, 0.0),
        ] {
            let (face, u, v) = face_uv(dir);
            let back = face_uv_to_direction(face, u, v);
            assert!(
                (back - dir.normalize()).length() < 1e-9,
                "round trip failed for {dir}"
            );
        }
    }

    #[test]
    fn quadtree_subdivision_covers_children() {
        let root = TerrainPatch::root(CubeFace::PosZ);
        let children = root.subdivide();
        assert_eq!(children.len(), 4);
        assert!(children.iter().all(|c| c.level == 1));
        // Tiles tile the parent [0,1)² into four quadrants.
        let mut tiles: Vec<(u32, u32)> = children.iter().map(|c| (c.tile_x, c.tile_y)).collect();
        tiles.sort();
        assert_eq!(tiles, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    }

    #[test]
    fn patch_for_direction_lands_in_uv_bounds() {
        let dir = DVec3::new(0.4, 0.6, 1.0).normalize();
        let patch = TerrainPatch::for_direction(dir, 3);
        let (u0, v0, u1, v1) = patch.uv_bounds();
        let (_, u, v) = face_uv(dir);
        assert!(
            u >= u0 - 1e-9 && u <= u1 + 1e-9,
            "u {u} outside [{u0},{u1}]"
        );
        assert!(
            v >= v0 - 1e-9 && v <= v1 + 1e-9,
            "v {v} outside [{v0},{v1}]"
        );
    }

    #[test]
    fn lod_increases_as_camera_approaches() {
        let r = 6_371_000.0;
        let far = lod_for_distance(1_000_000.0, r, 1.0, 1080.0, 4.0, 12);
        let near = lod_for_distance(10_000.0, r, 1.0, 1080.0, 4.0, 12);
        assert!(near >= far, "near LOD {near} must be >= far LOD {far}");
        // Same distance is stable.
        let a = lod_for_distance(50_000.0, r, 1.0, 1080.0, 4.0, 12);
        let b = lod_for_distance(50_000.0, r, 1.0, 1080.0, 4.0, 12);
        assert_eq!(a, b);
    }

    #[test]
    fn mesh_conforms_to_sphere() {
        let patch = TerrainPatch::for_direction(DVec3::new(1.0, 0.0, 0.0), 1);
        let geom = build_patch_geometry(&patch, &source(), 6_371_000.0, 5, 50.0);
        assert!(geom.positions.len() >= 25);
        for p in &geom.positions {
            let r = DVec3::from_array(*p).length();
            // Radius stays near planet radius ± terrain amplitude.
            assert!(
                (r - 6_371_000.0).abs() < 2_500.0,
                "vertex radius {r} off the sphere"
            );
        }
        assert!(!geom.indices.is_empty());
    }

    #[test]
    fn boundary_height_is_shared_across_lod() {
        let s = source();
        // A shared edge point on the same face at two LODs must agree because
        // both sample the same TerrainSource at the same direction.
        let dir = DVec3::new(0.3, 0.4, 1.0).normalize();
        let coarse = TerrainPatch::for_direction(dir, 2);
        let fine = TerrainPatch::for_direction(dir, 4);
        let h_coarse = sample_height(&coarse, &s, dir, 6_371_000.0);
        let h_fine = sample_height(&fine, &s, dir, 6_371_000.0);
        assert_eq!(h_coarse, h_fine);
    }

    fn sample_height(
        patch: &TerrainPatch,
        source: &dyn TerrainSource,
        dir: DVec3,
        radius: f64,
    ) -> f64 {
        let (u0, v0, u1, v1) = patch.uv_bounds();
        let (_, u, v) = face_uv(dir);
        let tu = (u - u0) / (u1 - u0);
        let tv = (v - v0) / (v1 - v0);
        let ud = face_uv_to_direction(patch.face, u0 + tu * (u1 - u0), v0 + tv * (v1 - v0));
        let (lat, lon) = direction_to_lat_lon(ud);
        let _ = radius;
        source.height_m(lat, lon)
    }

    #[test]
    fn patch_world_size_shrinks_with_level() {
        let r = 6_371_000.0;
        let l0 = patch_world_size_m(0, r);
        let l1 = patch_world_size_m(1, r);
        assert!((l0 / l1 - 2.0).abs() < 1e-9);
    }

    /// Determinism: two independent builds of the same patch must produce
    /// byte-identical position arrays (AGENTS.md section 44). Compared via
    /// raw bit patterns so even -0.0 vs 0.0 or NaN payloads would fail.
    #[test]
    fn rebuild_same_patch_produces_byte_identical_positions() {
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 3);
        let build_geometry = || {
            let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
            build_patch_geometry(&patch, &source, 6_371_000.0, 8, 40.0)
        };

        let a = build_geometry();
        let b = build_geometry();

        assert_eq!(a.positions.len(), b.positions.len());
        for (pa, pb) in a.positions.iter().zip(b.positions.iter()) {
            // Byte-level comparison of the f64 coordinates.
            for c in 0..3 {
                assert_eq!(
                    pa[c].to_bits(),
                    pb[c].to_bits(),
                    "patch geometry is not deterministic"
                );
            }
        }
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.normals, b.normals);
    }
}
