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
    direction_to_lat_lon, face_uv_to_direction, PatchGeometry, TerrainPatch,
};
use crate::domain::services::terrain_collision::surface_normal;
use crate::domain::services::terrain_source::{
    slope_deg_at, surface_appearance, with_river_appearance, TerrainSource,
};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::DVec3;
use bevy::prelude::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_mesh::{Indices, Mesh, PrimitiveTopology};

/// Maximum scatter counts for a level-12 patch. Finer leaves scale their count
/// by patch area so refinement preserves density rather than multiplying it.
const TREE_COUNT: usize = 64;
/// A bounded carpet of crossed blades makes close vegetation read as grass
/// without adding entities or unique materials.
const GRASS_CLUMP_COUNT: usize = 512;
const ROCK_COUNT: usize = 14;
const TRUNK_SEGMENTS: usize = 6;
const FOLIAGE_SEGMENTS: usize = 7;
const BOULDER_SEGMENTS: usize = 6;
const BOULDER_RINGS: usize = 3;
const VEGETATION_BYTES_PER_VERTEX: u64 = 40;
const VEGETATION_BYTES_PER_INDEX: u64 = 4;

/// Vegetation is only useful on the finest terrain tiles. Generating it for
/// coarse parent coverage spends CPU, GPU memory, and draw calls on scatter
/// that is too distant to resolve.
pub(crate) const VEGETATION_MIN_PATCH_LEVEL: u32 = 12;

/// Local surface maps are reserved for the same close-range terrain level as
/// vegetation so their source sampling and GPU residency stay bounded.
pub(crate) const LOCAL_SURFACE_MIN_PATCH_LEVEL: u32 = VEGETATION_MIN_PATCH_LEVEL;

/// Albedo and normal maps are both RGBA8 textures.
pub(crate) const LOCAL_SURFACE_MAP_BYTES: u64 = SURFACE_TEX_RES as u64 * SURFACE_TEX_RES as u64 * 8;

pub(crate) fn supports_vegetation(patch_level: u32) -> bool {
    patch_level >= VEGETATION_MIN_PATCH_LEVEL
}

pub(crate) fn supports_local_surfaces(patch_level: u32) -> bool {
    patch_level >= LOCAL_SURFACE_MIN_PATCH_LEVEL
}

/// Source-derived patch data built by the streaming worker and consumed once by
/// the render upload path. Keeping it here prevents asset upload from sampling
/// a DEM, triggering erosion, or generating scatter on the presentation thread.
pub(crate) struct PreparedPatchSurface {
    pub vertex_colors: Vec<[f32; 4]>,
    pub roughness: f32,
    pub metallic: f32,
    pub local_surfaces: Option<(Image, Image)>,
    pub vegetation: Option<(Mesh, DVec3)>,
}

pub(crate) fn prepare_patch_surface(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    geometry: &PatchGeometry,
    radius_m: f64,
) -> PreparedPatchSurface {
    // Global Earth albedo is the macro terrain color at every LOD. Fine patches
    // add one local modulation map; vertex colors must stay neutral so Bevy's
    // StandardMaterial path cannot multiply the same biome signal twice.
    let uses_erosion_surface_data = supports_local_surfaces(patch.level);
    let vertex_colors = vec![[1.0, 1.0, 1.0, 1.0]; geometry.positions.len()];

    let center = patch.center_direction();
    let (lat, lon) = direction_to_lat_lon(center);
    let height_m = source.mesh_height_m(lat, lon, patch.level);
    let moisture = if uses_erosion_surface_data {
        source.moisture(lat, lon)
    } else {
        source.overview_moisture(lat, lon)
    };
    let slope_deg = if uses_erosion_surface_data {
        slope_deg_at(source, lat, lon)
    } else {
        source.overview_slope_deg(lat, lon)
    };
    let river_strength = if uses_erosion_surface_data {
        source.river_strength(lat, lon)
    } else {
        0.0
    };
    let appearance = with_river_appearance(
        surface_appearance(height_m, moisture, source.zone_lat(lat), slope_deg),
        river_strength,
    );
    let local_surfaces = supports_local_surfaces(patch.level)
        .then(|| build_patch_surfaces(source, patch, geometry, radius_m));
    let vegetation_anchor = center * (radius_m + height_m);
    let vegetation = supports_vegetation(patch.level)
        .then(|| build_vegetation_mesh(source, patch, radius_m, &vegetation_anchor))
        .flatten()
        .map(|mesh| (mesh, vegetation_anchor));
    PreparedPatchSurface {
        vertex_colors,
        roughness: appearance.roughness,
        metallic: appearance.metallic,
        local_surfaces,
        vegetation,
    }
}

/// Conservative maximum allocation for one merged vegetation mesh. The
/// streaming budget uses this for vegetation-eligible patches before it knows
/// their biome.
pub(crate) const MAX_VEGETATION_MESH_BYTES: u64 = {
    let tree_vertices = 2 * (TRUNK_SEGMENTS + 1) + 2 * (FOLIAGE_SEGMENTS + 1);
    let tree_indices = TRUNK_SEGMENTS * 6 + FOLIAGE_SEGMENTS * 6;
    let boulder_vertices = (BOULDER_RINGS + 1) * BOULDER_SEGMENTS;
    let boulder_indices = BOULDER_RINGS * BOULDER_SEGMENTS * 6;
    let grass_vertices = 6;
    let grass_indices = 6;
    let vertices = TREE_COUNT * tree_vertices
        + ROCK_COUNT * boulder_vertices
        + GRASS_CLUMP_COUNT * grass_vertices;
    let indices = TREE_COUNT * tree_indices
        + ROCK_COUNT * boulder_indices
        + GRASS_CLUMP_COUNT * grass_indices;
    vertices as u64 * VEGETATION_BYTES_PER_VERTEX + indices as u64 * VEGETATION_BYTES_PER_INDEX
};

/// Texture resolution (texels per side) for close-patch surface maps. At the
/// finest ~10 km Earth tiles this retains material detail below 80 m per texel
/// without changing authoritative geometry or collision sampling.
const SURFACE_TEX_RES: u32 = 128;
/// Blend a restrained amount of source micro-normal into the rendered mesh
/// normal. Macro slopes remain in mesh geometry; this map only adds grain.
const NORMAL_DETAIL_WEIGHT: f64 = 0.2;

/// Deterministic pseudo-noise used only to vary scatter silhouettes. Terrain
/// color comes exclusively from the shared `surface_appearance` authority.
fn micro_noise(x: f64, y: f64, z: f64) -> f64 {
    let s = x.sin() * 12.9898 + y.sin() * 78.233 + z.sin() * 37.719;
    s - s.floor()
}

/// Build the per-patch albedo + residual tangent-space normal map from the
/// shared source. The normal map represents source detail missing from the
/// existing patch mesh rather than applying the complete terrain slope twice.
pub fn build_patch_surfaces(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    geometry: &PatchGeometry,
    radius_m: f64,
) -> (Image, Image) {
    let res = SURFACE_TEX_RES as usize;
    let (u0, v0, u1, v1) = patch.uv_bounds();

    // Source samples remain worker-local. Their high-resolution normals provide
    // only residual detail relative to the already-generated terrain mesh.
    let mut h = vec![0.0f64; res * res];
    let mut lat = vec![0.0f64; res * res];
    let mut lon = vec![0.0f64; res * res];
    let mut positions = vec![DVec3::ZERO; res * res];
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
            positions[idx] = dir * (radius_m + h[idx]);
        }
    }

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
            let (source_tangent_u, source_tangent_v) = grid_tangents(&positions, res, i, j);
            let source_normal =
                outward_normal(source_tangent_u.cross(source_tangent_v), positions[idx]);
            let (mesh_tangent_u, mesh_tangent_v, mesh_normal) =
                mesh_surface_frame(geometry, i, j, res);
            let source_slope_deg = source_normal
                .dot(positions[idx].normalize())
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
            let appearance = with_river_appearance(
                surface_appearance(hi, moisture, zone, source_slope_deg),
                source.river_strength(la, lo),
            );

            // The global catalog albedo owns macro geography. Local samples
            // encode only a bounded linear material modulation, so local detail
            // enriches that imagery instead of replacing it with procedural tan.
            let [r, g, b, _] = terrain_albedo_modulation(appearance);
            albedo.extend_from_slice(&[
                (r * 255.0) as u8,
                (g * 255.0) as u8,
                (b * 255.0) as u8,
                255,
            ]);

            let tangent = (mesh_tangent_u - mesh_normal * mesh_normal.dot(mesh_tangent_u))
                .normalize_or_zero();
            let mut bitangent = mesh_normal.cross(tangent).normalize_or_zero();
            if bitangent.dot(mesh_tangent_v) < 0.0 {
                bitangent = -bitangent;
            }
            let detailed_normal = (mesh_normal * (1.0 - NORMAL_DETAIL_WEIGHT)
                + source_normal * NORMAL_DETAIL_WEIGHT)
                .normalize_or_zero();
            let local_normal = DVec3::new(
                detailed_normal.dot(tangent),
                detailed_normal.dot(bitangent),
                detailed_normal.dot(mesh_normal),
            )
            .normalize_or_zero();
            normal_data.extend_from_slice(&[
                ((local_normal.x * 0.5 + 0.5) * 255.0).round() as u8,
                ((local_normal.y * 0.5 + 0.5) * 255.0).round() as u8,
                ((local_normal.z * 0.5 + 0.5) * 255.0).round() as u8,
                (appearance.roughness.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }

    let extent = Extent3d {
        width: res as u32,
        height: res as u32,
        depth_or_array_layers: 1,
    };
    let mut albedo_img = Image::new(
        extent,
        TextureDimension::D2,
        albedo,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut normal_img = Image::new(
        extent,
        TextureDimension::D2,
        normal_data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    let sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    });
    albedo_img.sampler = sampler.clone();
    normal_img.sampler = sampler;
    (albedo_img, normal_img)
}

/// Convert procedural biome data into a restrained linear darkening multiplier
/// for the Earth albedo. Keeping the range within UNorm avoids clipped local
/// texture data and leaves catalog imagery as the dominant terrain appearance.
fn terrain_albedo_modulation(
    appearance: crate::domain::services::terrain_source::SurfaceAppearance,
) -> [f32; 4] {
    let map_channel = |channel: f32| 0.86 + channel.clamp(0.0, 1.0) * 0.14;
    [
        map_channel(appearance.albedo[0]),
        map_channel(appearance.albedo[1]),
        map_channel(appearance.albedo[2]),
        1.0,
    ]
}

fn grid_tangents(points: &[DVec3], resolution: usize, i: usize, j: usize) -> (DVec3, DVec3) {
    let idx = |x: usize, y: usize| y * resolution + x;
    let tangent_u = if i == 0 {
        points[idx(1, j)] - points[idx(0, j)]
    } else if i + 1 == resolution {
        points[idx(i, j)] - points[idx(i - 1, j)]
    } else {
        points[idx(i + 1, j)] - points[idx(i - 1, j)]
    };
    let tangent_v = if j == 0 {
        points[idx(i, 1)] - points[idx(i, 0)]
    } else if j + 1 == resolution {
        points[idx(i, j)] - points[idx(i, j - 1)]
    } else {
        points[idx(i, j + 1)] - points[idx(i, j - 1)]
    };
    (tangent_u, tangent_v)
}

fn outward_normal(normal: DVec3, position: DVec3) -> DVec3 {
    let normal = normal.normalize_or_zero();
    if normal.dot(position) < 0.0 {
        -normal
    } else {
        normal
    }
}

fn mesh_surface_frame(
    geometry: &PatchGeometry,
    texture_i: usize,
    texture_j: usize,
    texture_resolution: usize,
) -> (DVec3, DVec3, DVec3) {
    // Grid vertices precede the skirt ring. Its count solves
    // `vertices = resolution^2 + 4 * (resolution - 1)`.
    let mesh_resolution = ((geometry.positions.len() + 8) as f64).sqrt() as usize - 2;
    let mesh_extent = (mesh_resolution - 1) as f64;
    let u = texture_i as f64 / (texture_resolution - 1) as f64 * mesh_extent;
    let v = texture_j as f64 / (texture_resolution - 1) as f64 * mesh_extent;
    let i = u.floor().min(mesh_extent - 1.0) as usize;
    let j = v.floor().min(mesh_extent - 1.0) as usize;
    let fu = u - i as f64;
    let fv = v - j as f64;
    let point = |x: usize, y: usize| DVec3::from_array(geometry.positions[y * mesh_resolution + x]);
    let p00 = point(i, j);
    let p10 = point(i + 1, j);
    let p01 = point(i, j + 1);
    let p11 = point(i + 1, j + 1);
    let tangent_u = (p10 - p00) * (1.0 - fv) + (p11 - p01) * fv;
    let tangent_v = (p01 - p00) * (1.0 - fu) + (p11 - p10) * fu;
    let position = (p00 * (1.0 - fu) + p10 * fu) * (1.0 - fv) + (p01 * (1.0 - fu) + p11 * fu) * fv;
    let normal = |x: usize, y: usize| DVec3::from_array(geometry.normals[y * mesh_resolution + x]);
    let mesh_normal = (normal(i, j) * (1.0 - fu) + normal(i + 1, j) * fu) * (1.0 - fv)
        + (normal(i, j + 1) * (1.0 - fu) + normal(i + 1, j + 1) * fu) * fv;
    (tangent_u, tangent_v, outward_normal(mesh_normal, position))
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
    #[expect(
        clippy::too_many_arguments,
        reason = "The mesh helper accepts the complete prism geometry and material inputs."
    )]
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
        let ref_axis = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
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
            // A tapered prism normal needs an axial component. A radial-only
            // cone normal produces a visibly incorrect highlight.
            let side_normal = (radial + up * ((r0 - r1) / height.max(f64::EPSILON))).normalize();
            self.positions.push([p0.x as f32, p0.y as f32, p0.z as f32]);
            self.positions.push([p1.x as f32, p1.y as f32, p1.z as f32]);
            self.normals.push([
                side_normal.x as f32,
                side_normal.y as f32,
                side_normal.z as f32,
            ]);
            self.normals.push([
                side_normal.x as f32,
                side_normal.y as f32,
                side_normal.z as f32,
            ]);
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
        let ref_axis = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let tangent = up.cross(ref_axis).normalize();
        let bitangent = up.cross(tangent).normalize();
        let segments = BOULDER_SEGMENTS;
        let rings = BOULDER_RINGS;
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

    fn push_grass_clump(
        &mut self,
        base: DVec3,
        up: DVec3,
        width_m: f64,
        height_m: f64,
        rotation_rad: f64,
        color: [f32; 3],
    ) {
        let reference = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let tangent = up.cross(reference).normalize();
        let bitangent = up.cross(tangent).normalize();
        for angle in [rotation_rad, rotation_rad + std::f64::consts::FRAC_PI_2] {
            let across = tangent * angle.cos() + bitangent * angle.sin();
            let start = self.positions.len() as u32;
            let left = base - across * width_m * 0.5;
            let right = base + across * width_m * 0.5;
            let tip = base + up * height_m;
            let normal = across.cross(up).normalize();
            for point in [left, right, tip] {
                self.positions
                    .push([point.x as f32, point.y as f32, point.z as f32]);
                self.normals
                    .push([normal.x as f32, normal.y as f32, normal.z as f32]);
                self.colors.push([color[0], color[1], color[2], 1.0]);
            }
            self.indices
                .extend_from_slice(&[start, start + 1, start + 2]);
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
    let mut h =
        a ^ (b.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ (c.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h & 0xFFFF_FFFF_FFFF) as f64 / 0x1_0000_0000_0000u64 as f64
}

/// Preserve scatter density as a quadtree patch splits into four children.
/// A minimum of one instance retains rocks and landmark vegetation in the
/// closest patches without creating an LOD-dependent density jump.
fn scatter_count_for_level(max_count: usize, patch_level: u32) -> usize {
    let level_delta = patch_level.saturating_sub(VEGETATION_MIN_PATCH_LEVEL);
    max_count
        .checked_shr(level_delta.saturating_mul(2))
        .unwrap_or(0)
        .max(1)
}

/// Build the merged vegetation/scatter mesh for a patch, or `None` if the patch
/// is not vegetated (water, snow, bare rock, or too steep). Positions are in the
/// rocket-local flight frame (planet-centered minus `render_origin`), matching
/// the terrain mesh so plants sit exactly on the surface.
pub fn build_vegetation_mesh(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    radius_m: f64,
    mesh_origin_body_fixed: &DVec3,
) -> Option<Mesh> {
    let (u0, v0, u1, v1) = patch.uv_bounds();

    let mut accum = MeshAccum::new();

    for k in 0..scatter_count_for_level(TREE_COUNT, patch.level) {
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
        let appearance = surface_appearance(
            h,
            source.moisture(lat, lon),
            source.zone_lat(lat),
            local_slope,
        );
        if appearance.albedo[1] <= appearance.albedo[0]
            || appearance.albedo[1] <= appearance.albedo[2]
        {
            continue;
        }
        let flight = dir * (radius_m + h) - *mesh_origin_body_fixed;
        let up = surface_normal(source, lat, lon, radius_m);
        // Vary tree size a little.
        let scale = 0.7 + hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64) * 0.9;
        let trunk_h = 2.2 * scale;
        let trunk_r = 0.18 * scale;
        let foliage_h = 4.5 * scale;
        let foliage_r = 1.6 * scale;
        let base = flight;
        let foliage_tint = 0.8 + hash01(k as u64, patch.tile_y as u64, patch.tile_x as u64) * 0.3;
        let trunk_color = [0.16f32, 0.07f32, 0.025f32];
        let foliage_color = [
            0.035f32 * foliage_tint as f32,
            0.19f32 * foliage_tint as f32,
            0.022f32 * foliage_tint as f32,
        ];
        accum.push_prism(
            base,
            up,
            trunk_r,
            trunk_r * 0.8,
            trunk_h,
            TRUNK_SEGMENTS,
            trunk_color,
        );
        let foliage_base = base + up * trunk_h;
        accum.push_prism(
            foliage_base,
            up,
            foliage_r,
            0.0,
            foliage_h,
            FOLIAGE_SEGMENTS,
            foliage_color,
        );
    }

    for k in 0..scatter_count_for_level(GRASS_CLUMP_COUNT, patch.level) {
        let ru = hash01(
            patch.face as u64 ^ 0xCAFE_BABE,
            patch.tile_x as u64,
            (patch.tile_y as u64).wrapping_add(k as u64),
        );
        let rv = hash01(
            patch.level as u64,
            patch.tile_y as u64 ^ 0x0A11_CE55,
            (patch.tile_x as u64).wrapping_add(k as u64),
        );
        let u = u0 + (u1 - u0) * ru;
        let v = v0 + (v1 - v0) * rv;
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.height_m(lat, lon);
        let slope_deg = slope_deg_at(source, lat, lon);
        let appearance = surface_appearance(
            h,
            source.moisture(lat, lon),
            source.zone_lat(lat),
            slope_deg,
        );
        if h < 0.5
            || h > 2_800.0
            || slope_deg > 28.0
            || appearance.albedo[1] <= appearance.albedo[0]
            || appearance.albedo[1] <= appearance.albedo[2]
        {
            continue;
        }
        let base = dir * (radius_m + h) - *mesh_origin_body_fixed;
        let up = surface_normal(source, lat, lon, radius_m);
        let scale = 0.55 + hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64) * 0.65;
        let grass_color = [0.045, 0.24, 0.025];
        accum.push_grass_clump(
            base,
            up,
            0.22 * scale,
            0.45 * scale,
            hash01(patch.tile_x as u64, patch.tile_y as u64, k as u64) * std::f64::consts::TAU,
            grass_color,
        );
    }

    for k in 0..scatter_count_for_level(ROCK_COUNT, patch.level) {
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
        let flight = dir * (radius_m + h) - *mesh_origin_body_fixed;
        let radius = 0.4 + hash01(k as u64, patch.tile_x as u64, 3) * 0.9;
        accum.push_boulder(
            flight,
            dir,
            radius,
            0x1234_5678 ^ (k as u64 * 2_654_355_561),
        );
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
    use crate::domain::services::cube_sphere::build_patch_geometry;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;

    #[derive(Debug)]
    struct RiverTerrain;

    impl TerrainSource for RiverTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            300.0
        }

        fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.5
        }

        fn river_strength(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            1.0
        }
    }

    #[test]
    fn patch_surfaces_produce_aligned_rgb_textures() {
        let src = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let geometry = build_patch_geometry(&patch, &src, 6_371_000.0, 33, 5.0);
        let (albedo, normal) = build_patch_surfaces(&src, &patch, &geometry, 6_371_000.0);
        assert_eq!(albedo.width(), SURFACE_TEX_RES);
        assert_eq!(albedo.height(), SURFACE_TEX_RES);
        assert_eq!(normal.width(), SURFACE_TEX_RES);
        assert_eq!(albedo.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        // 4 channels per texel.
        assert_eq!(
            albedo.data.as_ref().unwrap().len(),
            (SURFACE_TEX_RES as usize).pow(2) * 4
        );
        assert_eq!(normal.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        for image in [&albedo, &normal] {
            let ImageSampler::Descriptor(sampler) = &image.sampler else {
                panic!("local terrain maps must use explicit filtered sampling");
            };
            assert_eq!(sampler.mag_filter, ImageFilterMode::Linear);
            assert_eq!(sampler.min_filter, ImageFilterMode::Linear);
            assert_eq!(sampler.mipmap_filter, ImageFilterMode::Linear);
            assert_eq!(sampler.anisotropy_clamp, 16);
        }

        let (repeat_albedo, repeat_normal) =
            build_patch_surfaces(&src, &patch, &geometry, 6_371_000.0);
        assert_eq!(albedo.data, repeat_albedo.data);
        assert_eq!(normal.data, repeat_normal.data);
    }

    #[test]
    fn terrain_modulation_is_subtle_and_representable_as_unorm() {
        let grass = terrain_albedo_modulation(surface_appearance(300.0, 0.6, 0.5, 5.0));
        let rock = terrain_albedo_modulation(surface_appearance(1_500.0, 0.4, 0.5, 60.0));

        for channel in grass.into_iter().chain(rock) {
            assert!((0.86..=1.0).contains(&channel));
        }
        assert!(grass[1] > grass[0]);
        assert!(rock[0] > grass[0]);
    }

    #[test]
    fn river_strength_is_encoded_in_the_local_surface_map() {
        let source = RiverTerrain;
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 12);
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 33, 5.0);
        let (albedo, normal) = build_patch_surfaces(&source, &patch, &geometry, 6_371_000.0);
        let center = ((SURFACE_TEX_RES as usize / 2) * SURFACE_TEX_RES as usize
            + SURFACE_TEX_RES as usize / 2)
            * 4;
        let albedo = albedo.data.as_ref().unwrap();
        let normal = normal.data.as_ref().unwrap();

        assert!(albedo[center + 2] > albedo[center + 1]);
        assert!(
            normal[center + 3] < 100,
            "river channels must be smoother than ground"
        );
    }

    #[test]
    fn residual_normal_is_neutral_for_a_flat_source() {
        let source = ProceduralTerrainSource::new(0, 0.0, 0.0, 0);
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 12);
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 33, 5.0);
        let (_, normal) = build_patch_surfaces(&source, &patch, &geometry, 6_371_000.0);
        let data = normal.data.as_ref().unwrap();
        let center = ((SURFACE_TEX_RES as usize / 2) * SURFACE_TEX_RES as usize
            + SURFACE_TEX_RES as usize / 2)
            * 4;

        assert!((i16::from(data[center]) - 128).abs() <= 2);
        assert!((i16::from(data[center + 1]) - 128).abs() <= 2);
        assert!(data[center + 2] >= 252);
    }

    #[test]
    fn residual_normal_captures_detail_missing_from_a_coarse_mesh() {
        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 12);
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 33, 5.0);
        let (_, normal) = build_patch_surfaces(&source, &patch, &geometry, 6_371_000.0);
        let data = normal.data.as_ref().unwrap();
        let (texels, _) = data.as_chunks::<4>();

        assert!(
            texels.iter().any(|texel| {
                (i16::from(texel[0]) - 128).abs() > 0 || (i16::from(texel[1]) - 128).abs() > 0
            }),
            "a detailed source must encode non-neutral tangent-space normals for shader lighting"
        );
    }

    #[test]
    fn residual_normal_frame_uses_rendered_mesh_normals() {
        let geometry = crate::domain::services::cube_sphere::PatchGeometry {
            positions: vec![
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
            ],
            // Deliberately differ from the planar triangle normal. The map
            // frame must match the normal carried by the rendered mesh.
            normals: vec![[1.0, 0.0, 1.0]; 4],
            uvs: vec![],
            local_uvs: vec![],
            indices: vec![],
        };

        let (_, _, normal) = mesh_surface_frame(&geometry, 0, 0, 2);
        assert!(normal.dot(DVec3::new(1.0, 0.0, 1.0).normalize()) > 0.999_999);
    }

    #[test]
    fn vertex_color_is_neutral_across_an_adjacent_lod_boundary() {
        use crate::domain::services::cube_sphere::CubeFace;

        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let coarse = TerrainPatch {
            face: CubeFace::PosZ,
            level: 1,
            tile_x: 0,
            tile_y: 0,
        };
        // This child is the lower half of coarse patch's east neighbor, so its
        // west edge shares the coarse patch's east edge.
        let fine = TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 2,
            tile_y: 0,
        };
        let coarse_geometry = build_patch_geometry(&coarse, &source, 6_371_000.0, 5, 5.0);
        let fine_geometry = build_patch_geometry(&fine, &source, 6_371_000.0, 5, 5.0);
        let coarse_surface = prepare_patch_surface(&source, &coarse, &coarse_geometry, 6_371_000.0);
        let fine_surface = prepare_patch_surface(&source, &fine, &fine_geometry, 6_371_000.0);

        for fine_j in [0, 2, 4] {
            let coarse_j = fine_j / 2;
            assert_eq!(
                coarse_surface.vertex_colors[coarse_j * 5 + 4],
                fine_surface.vertex_colors[fine_j * 5],
                "the shared source appearance must not depend on mesh LOD"
            );
        }
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

    #[test]
    fn vegetation_is_limited_to_the_finest_terrain_tiles() {
        assert!(!supports_vegetation(VEGETATION_MIN_PATCH_LEVEL - 1));
        assert!(supports_vegetation(VEGETATION_MIN_PATCH_LEVEL));
    }

    #[test]
    fn scatter_density_stays_stable_as_patches_refine() {
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL),
            64
        );
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL + 1),
            16
        );
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL + 2),
            4
        );
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL + 3),
            1
        );
        assert_eq!(
            scatter_count_for_level(ROCK_COUNT, VEGETATION_MIN_PATCH_LEVEL + 2),
            1
        );
    }
}
