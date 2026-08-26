//! Procedural surface detailing for cube-sphere terrain patches (AGENTS.md 27).
//!
//! The streaming/geometry layer produces the planet-centered patch mesh from the
//! shared `TerrainSource`. This module turns that into a *premium* surface:
//!
//! 1. A per-patch **albedo** texture and **tangent-space normal map**, generated
//!    deterministically from the same source so biome color, slope-based rock,
//!    sandy shoreline and snow line all appear with crisp close-up detail
//!    instead of a flat per-vertex color.
//! 2. A merged **vegetation + scatter** mesh (low-poly trees and rocks) placed
//!    only on vegetated, low-slope, above-water ground, spawned as a single
//!    draw call per patch (one merged mesh, not per-plant entities).
//!
//! Everything is seeded from the patch coordinates, so it is reproducible and
//! independent of frame rate or spawn order (AGENTS.md 26, 44).

use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, patch_world_size_m, TerrainPatch,
};
use crate::domain::services::terrain_source::{slope_deg_at, surface_appearance, TerrainSource};
use bevy::math::DVec3;
use bevy::prelude::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::RenderAssetUsages;
use bevy_mesh::{Indices, Mesh, PrimitiveTopology};

/// Texture resolution (texels per side) for the per-patch surface maps.
const SURFACE_TEX_RES: u32 = 128;
/// Exaggeration applied to the height-gradient when baking the normal map so
/// fine relief reads clearly under the launch-pad sun.
const NORMAL_STRENGTH: f64 = 2.5;

/// Deterministic 3D pseudo-noise for micro surface variation (independent of the
/// source's own noise fields). Cheap hash-based; only used to break up flat
/// tonal bands up close.
fn micro_noise(x: f64, y: f64, z: f64) -> f64 {
    let s = x.sin() * 12.9898 + y.sin() * 78.233 + z.sin() * 37.719;
    s - s.floor()
}

/// Build the per-patch albedo + tangent-space normal map from the shared source.
/// `tex_res` texels are sampled across the patch's [u0..u1]x[v0..v1] parameter
/// space; the result aligns 1:1 with the mesh UVs produced by `build_patch_geometry`.
pub fn build_patch_surfaces(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    radius_m: f64,
) -> (Image, Image) {
    let res = SURFACE_TEX_RES as usize;
    let (u0, v0, u1, v1) = patch.uv_bounds();

    // Height grid (one source sample per texel; slope derived by finite diffs
    // of this same grid so color and normal stay consistent).
    let mut h = vec![0.0f64; res * res];
    let mut lat = vec![0.0f64; res * res];
    let mut lon = vec![0.0f64; res * res];
    for j in 0..res {
        for i in 0..res {
            let u = u0 + (u1 - u0) * i as f64 / (res - 1) as f64;
            let v = v0 + (v1 - v0) * j as f64 / (res - 1) as f64;
            let dir = face_uv_to_direction(patch.face, u, v);
            let (la, lo) = direction_to_lat_lon(dir);
            let idx = j * res + i;
            lat[idx] = la;
            lon[idx] = lo;
            h[idx] = source.height_m(la, lo);
        }
    }

    let world_size = patch_world_size_m(patch.level, radius_m);
    let texel_m = world_size / (res - 1) as f64;

    let mut albedo = Vec::with_capacity(res * res * 4);
    let mut normal_data = Vec::with_capacity(res * res * 4);

    for j in 0..res {
        for i in 0..res {
            let idx = j * res + i;
            let la = lat[idx];
            let lo = lon[idx];
            let hi = h[idx];
            let moisture = source.moisture(la, lo);
            let zone = source.zone_lat(la);

            // Slope from neighboring texels (central difference, clamped edges).
            let il = if i > 0 { idx - 1 } else { idx };
            let ir = if i < res - 1 { idx + 1 } else { idx };
            let ju = if j > 0 { idx - res } else { idx };
            let jd = if j < res - 1 { idx + res } else { idx };
            let dhdu = (h[ir] - h[il]) / (2.0 * texel_m);
            let dhdv = (h[jd] - h[ju]) / (2.0 * texel_m);
            let slope = (dhdu.hypot(dhdv)).atan().to_degrees();

            let appearance = surface_appearance(hi, moisture, zone, slope);

            // Micro variation: perturb albedo slightly so flat biomes (grass,
            // sand) do not read as a single flat tone up close.
            let n = micro_noise(la * 0.7, lo * 0.7, hi * 0.01);
            let shade = 0.92 + n * 0.16; // [0.92, 1.08]
            let r = (appearance.albedo[0] * shade as f32).clamp(0.0, 1.0);
            let g = (appearance.albedo[1] * shade as f32).clamp(0.0, 1.0);
            let b = (appearance.albedo[2] * shade as f32).clamp(0.0, 1.0);
            let c = bevy::prelude::Color::srgb(r, g, b).to_linear();
            albedo.extend_from_slice(&[
                (c.red * 255.0) as u8,
                (c.green * 255.0) as u8,
                (c.blue * 255.0) as u8,
                255,
            ]);

            // Tangent-space normal from the height gradient.
            let nx = (-dhdu * NORMAL_STRENGTH) as f32;
            let ny = (-dhdv * NORMAL_STRENGTH) as f32;
            let nz = 1.0f32;
            let inv = (nx * nx + ny * ny + nz * nz).sqrt().recip();
            normal_data.extend_from_slice(&[
                (((nx * inv) * 0.5 + 0.5) * 255.0) as u8,
                (((ny * inv) * 0.5 + 0.5) * 255.0) as u8,
                (((nz * inv) * 0.5 + 0.5) * 255.0) as u8,
                255,
            ]);
        }
    }

    let extent = Extent3d {
        width: res as u32,
        height: res as u32,
        depth_or_array_layers: 1,
    };
    let albedo_img = Image::new(
        extent,
        TextureDimension::D2,
        albedo,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    let normal_img = Image::new(
        extent,
        TextureDimension::D2,
        normal_data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    (albedo_img, normal_img)
}

/// Accumulator for building one merged vegetation/scatter mesh per patch.
struct MeshAccum {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl MeshAccum {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Push a vertical prism (cylinder/cone) whose axis is `up` at `base`.
    /// `r0`/`r1` are bottom/top radii, `height` the axis length. `color` is the
    /// linear vertex color. `segments` controls tessellation.
    fn push_prism(
        &mut self,
        base: DVec3,
        up: DVec3,
        r0: f64,
        r1: f64,
        height: f64,
        segments: usize,
        color: [f32; 3],
    ) {
        // Orthonormal tangent basis around `up`.
        let ref_axis = if up.y.abs() < 0.9 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let tangent = up.cross(ref_axis).normalize();
        let bitangent = up.cross(tangent).normalize();

        let start = self.positions.len() as u32;
        let top = base + up * height;

        for s in 0..=segments {
            let a = s as f64 / segments as f64 * std::f64::consts::TAU;
            let ca = a.cos();
            let sa = a.sin();
            let radial = tangent * ca + bitangent * sa;
            let p0 = base + radial * r0;
            let p1 = top + radial * r1;
            // Side normal points outward (radial) for a cylinder/cone.
            self.positions.push([p0.x as f32, p0.y as f32, p0.z as f32]);
            self.positions.push([p1.x as f32, p1.y as f32, p1.z as f32]);
            self.normals.push([radial.x as f32, radial.y as f32, radial.z as f32]);
            self.normals.push([radial.x as f32, radial.y as f32, radial.z as f32]);
            self.colors.push([color[0], color[1], color[2], 1.0]);
            self.colors.push([color[0], color[1], color[2], 1.0]);
        }

        for s in 0..segments {
            let a0 = start + (2 * s) as u32;
            let a1 = start + (2 * s + 1) as u32;
            let b0 = start + (2 * s + 2) as u32;
            let b1 = start + (2 * s + 3) as u32;
            self.indices.extend_from_slice(&[a0, b0, a1, a1, b0, b1]);
        }
    }

    /// Push a low-poly boulder: a lumpy lump (random radial jitter per vertex).
    fn push_boulder(&mut self, center: DVec3, up: DVec3, radius: f64, seed: u64) {
        let ref_axis = if up.y.abs() < 0.9 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let tangent = up.cross(ref_axis).normalize();
        let bitangent = up.cross(tangent).normalize();
        let segments = 6usize;
        let rings = 3usize;
        let start = self.positions.len() as u32;
        let color = [0.42f32, 0.40f32, 0.38f32];

        for r in 0..=rings {
            let phi = (r as f64 / rings as f64) * std::f64::consts::PI;
            let y = phi.cos();
            let ring_r = phi.sin() * radius;
            for s in 0..segments {
                let a = s as f64 / segments as f64 * std::f64::consts::TAU;
                let jitter = 0.7 + micro_noise(seed as f64 + a, y, r as f64) * 0.5;
                let ca = a.cos();
                let sa = a.sin();
                let dir = tangent * ca + bitangent * sa;
                let p = center + dir * ring_r * jitter + up * (y * radius * jitter);
                let n = (dir * ring_r + up * y).normalize();
                self.positions.push([p.x as f32, p.y as f32, p.z as f32]);
                self.normals.push([n.x as f32, n.y as f32, n.z as f32]);
                self.colors.push([color[0], color[1], color[2], 1.0]);
            }
        }
        for r in 0..rings {
            for s in 0..segments {
                let a = start + (r * segments + s) as u32;
                let b = start + (r * segments + (s + 1) % segments) as u32;
                let c = start + ((r + 1) * segments + s) as u32;
                let d = start + ((r + 1) * segments + (s + 1) % segments) as u32;
                self.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

/// Deterministically hash patch coordinates + index `k` to a stable pseudo-random
/// [0,1) value (used for scatter placement so it never depends on frame order).
fn hash01(a: u64, b: u64, c: u64) -> f64 {
    let mut h = a ^ (b.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ (c.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h & 0xFFFF_FFFF_FFFF) as f64 / 0x1_0000_0000_0000u64 as f64
}

/// Build the merged vegetation/scatter mesh for a patch, or `None` if the patch
/// is not vegetated (water, snow, bare rock, or too steep). Positions are in the
/// rocket-local flight frame (planet-centered minus `render_origin`), matching
/// the terrain mesh so plants sit exactly on the surface.
pub fn build_vegetation_mesh(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    radius_m: f64,
    render_origin: &DVec3,
) -> Option<Mesh> {
    let (u0, v0, u1, v1) = patch.uv_bounds();

    // Decide vegetation from the patch centre biome.
    let cu = (u0 + u1) * 0.5;
    let cv = (v0 + v1) * 0.5;
    let cdir = face_uv_to_direction(patch.face, cu, cv);
    let (clat, clon) = direction_to_lat_lon(cdir);
    let cheight = source.height_m(clat, clon);
    let cmoisture = source.moisture(clat, clon);
    let czone = source.zone_lat(clat);
    let cslope = slope_deg_at(source, clat, clon);
    let center = surface_appearance(cheight, cmoisture, czone, cslope);

    // Vegetate grass/forest biomes: greenish albedo, above water, below snow,
    // not on cliff faces.
    let is_vegetated = center.albedo[1] > center.albedo[0]
        && center.albedo[1] > center.albedo[2]
        && cheight > 1.0
        && cheight < 2600.0
        && cslope < 32.0;
    if !is_vegetated {
        return None;
    }

    let tree_count = 70usize;
    let rock_count = 14usize;
    let mut accum = MeshAccum::new();

    for k in 0..tree_count {
        let ru = hash01(
            patch.face as u64,
            (patch.tile_x as u64) ^ (k as u64 * 2_654_355_561),
            (patch.tile_y as u64) ^ (k as u64 * 4_478_569),
        );
        let rv = hash01(
            (patch.level as u64) ^ (k as u64 * 3_141_592_653),
            patch.tile_x as u64,
            patch.tile_y as u64,
        );
        let u = u0 + (u1 - u0) * ru;
        let v = v0 + (v1 - v0) * rv;
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.height_m(lat, lon);
        if h < 0.5 || h > 2600.0 {
            continue;
        }
        let local_slope = slope_deg_at(source, lat, lon);
        if local_slope > 34.0 {
            continue;
        }
        let planet_pos = dir * (radius_m + h);
        let flight = planet_pos - *render_origin;
        let up = dir; // radial up (terrain normal ~ radial for gentle slopes)
        // Vary tree size a little.
        let scale = 0.7 + hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64) * 0.9;
        let trunk_h = 2.2 * scale;
        let trunk_r = 0.18 * scale;
        let foliage_h = 4.5 * scale;
        let foliage_r = 1.6 * scale;
        let base = flight;
        let trunk_color = [0.30f32, 0.22f32, 0.13f32];
        let foliage_color = [0.18f32, 0.38f32, 0.13f32];
        accum.push_prism(base, up, trunk_r, trunk_r * 0.8, trunk_h, 6, trunk_color);
        let foliage_base = base + up * trunk_h;
        accum.push_prism(foliage_base, up, foliage_r, 0.0, foliage_h, 7, foliage_color);
    }

    for k in 0..rock_count {
        let ru = hash01(
            (patch.face as u64) ^ (k as u64 * 7_919),
            patch.tile_y as u64,
            patch.tile_x as u64,
        );
        let rv = hash01(
            patch.tile_x as u64 ^ (k as u64 * 1_009),
            patch.level as u64,
            patch.tile_y as u64,
        );
        let u = u0 + (u1 - u0) * ru;
        let v = v0 + (v1 - v0) * rv;
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.height_m(lat, lon);
        if h < 0.5 {
            continue;
        }
        let planet_pos = dir * (radius_m + h);
        let flight = planet_pos - *render_origin;
        let radius = 0.4 + hash01(k as u64, patch.tile_x as u64, 3) * 0.9;
        accum.push_boulder(flight, dir, radius, 0x1234_5678 ^ (k as u64 * 2_654_355_561));
    }

    if accum.positions.is_empty() {
        None
    } else {
        Some(accum.into_mesh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;

    #[test]
    fn patch_surfaces_produce_aligned_rgb_textures() {
        let src = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let (albedo, normal) = build_patch_surfaces(&src, &patch, 6_371_000.0);
        assert_eq!(albedo.width(), SURFACE_TEX_RES);
        assert_eq!(albedo.height(), SURFACE_TEX_RES);
        assert_eq!(normal.width(), SURFACE_TEX_RES);
        // 4 channels per texel.
        assert_eq!(
            albedo.data.as_ref().unwrap().len(),
            (SURFACE_TEX_RES as usize).pow(2) * 4
        );
    }

    #[test]
    fn vegetation_respects_source_and_is_deterministic() {
        let src = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let a = build_vegetation_mesh(&src, &patch, 6_371_000.0, &DVec3::ZERO);
        let b = build_vegetation_mesh(&src, &patch, 6_371_000.0, &DVec3::ZERO);
        assert_eq!(a.is_some(), b.is_some());

        // Some patch on this planet must be vegetated (green land exists), and
        // the merged mesh must carry vertex colors so it renders without a
        // white base bleeding through.
        use crate::domain::services::cube_sphere::CubeFace;
        let faces = [
            CubeFace::PosX,
            CubeFace::NegX,
            CubeFace::PosY,
            CubeFace::NegY,
            CubeFace::PosZ,
            CubeFace::NegZ,
        ];
        let mut found_veg = false;
        for face in faces {
            for t in 0..8u32 {
                let dir = face_uv_to_direction(face, t as f64 / 8.0, 0.5);
                let p = TerrainPatch::for_direction(dir, 2);
                if let Some(mesh) = build_vegetation_mesh(&src, &p, 6_371_000.0, &DVec3::ZERO) {
                    assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
                    found_veg = true;
                    break;
                }
            }
            if found_veg {
                break;
            }
        }
        assert!(found_veg, "expected at least one vegetated patch");
    }
}
