//! Per-patch surface maps: deterministic albedo, tangent-space normal, and
//! biome appearance derived from the shared authoritative `TerrainSource`.

use super::super::mips::{mip_chain_rgba8, MipFilter};
use super::{direction_to_lat_lon, face_uv_to_direction};
use crate::domain::services::cube_sphere::{PatchGeometry, TerrainPatch};
use crate::domain::services::terrain_source::{
    surface_appearance, with_river_appearance, TerrainSource,
};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::DVec3;
use bevy::prelude::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Texture resolution (texels per side) for close-patch surface maps. At the
/// finest ~10 km Earth tiles this retains material detail below 80 m per texel
/// without changing authoritative geometry or collision sampling.
pub(super) const SURFACE_TEX_RES: u32 = 128;
/// Blend a restrained amount of source micro-normal into the rendered mesh
/// normal. Macro slopes remain in mesh geometry; this map only adds grain.
const NORMAL_DETAIL_WEIGHT: f64 = 0.2;
const PAPUA_LATITUDE_MIN_DEG: f64 = -10.0;
const PAPUA_LATITUDE_MAX_DEG: f64 = 2.0;
const PAPUA_LONGITUDE_MIN_DEG: f64 = 128.0;
const PAPUA_LONGITUDE_MAX_DEG: f64 = 146.0;
const PAPUA_PROFILE_EDGE_DEG: f64 = 1.5;

/// Deterministic visual-only profile derived from the existing terrain source
/// samples. It is intentionally a broad regional approximation, not land-cover
/// data, and does not affect terrain geometry, collision, or streaming.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct PapuaTropicalProfile {
    pub(super) tropical_lowland_unit: f64,
    pub(super) wet_vegetation_unit: f64,
}

pub(super) fn papua_tropical_profile(
    latitude_deg: f64,
    longitude_deg: f64,
    height_m: f64,
    moisture_unit: f64,
    slope_deg: f64,
) -> PapuaTropicalProfile {
    let regional = interval_weight(latitude_deg, PAPUA_LATITUDE_MIN_DEG, PAPUA_LATITUDE_MAX_DEG)
        * interval_weight(
            longitude_deg,
            PAPUA_LONGITUDE_MIN_DEG,
            PAPUA_LONGITUDE_MAX_DEG,
        );
    let lowland = 1.0 - (height_m.max(0.0) / 1_400.0).clamp(0.0, 1.0);
    let gentle_ground = 1.0 - (slope_deg.max(0.0) / 38.0).clamp(0.0, 1.0);
    let wet = ((moisture_unit.clamp(0.0, 1.0) - 0.32) / 0.68).clamp(0.0, 1.0);
    PapuaTropicalProfile {
        tropical_lowland_unit: regional * lowland,
        wet_vegetation_unit: regional * lowland * gentle_ground * wet,
    }
}

pub(super) fn interval_weight(value: f64, min: f64, max: f64) -> f64 {
    let inside = (value - min).min(max - value);
    (inside / PAPUA_PROFILE_EDGE_DEG).clamp(0.0, 1.0)
}

/// Deterministic pseudo-noise used only to vary scatter silhouettes. Terrain
/// color comes exclusively from the shared `surface_appearance` authority.
pub(super) fn micro_noise(x: f64, y: f64, z: f64) -> f64 {
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

            let profile = papua_tropical_profile(la, lo, hi, moisture, source_slope_deg);
            let [r, g, b, _] = terrain_albedo(appearance, profile);
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
    // Bevy does not generate mipmaps, so supply the chain here. Without it the
    // mip-filtering sampler aliases the source detail into shimmer as soon as a
    // patch is minified.
    let (albedo, albedo_levels) =
        mip_chain_rgba8(res as u32, res as u32, &albedo, MipFilter::Color);
    let (normal_data, normal_levels) =
        mip_chain_rgba8(res as u32, res as u32, &normal_data, MipFilter::Normal);
    let mut albedo_img = Image::new_uninit(
        extent,
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    albedo_img.data = Some(albedo);
    albedo_img.texture_descriptor.mip_level_count = albedo_levels;
    let mut normal_img = Image::new_uninit(
        extent,
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    normal_img.data = Some(normal_data);
    normal_img.texture_descriptor.mip_level_count = normal_levels;
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

/// Convert the authoritative biome appearance into unpremultiplied linear color
/// for the terrain's complete source-derived albedo map.
pub(super) fn terrain_albedo(
    appearance: crate::domain::services::terrain_source::SurfaceAppearance,
    profile: PapuaTropicalProfile,
) -> [f32; 4] {
    // Papua's humid lowlands bias existing source-derived material toward wet,
    // deeply vegetated soil. This leaves slope/river/rock classification intact.
    let wet_soil = [0.055, 0.18, 0.045];
    let blend = profile.wet_vegetation_unit as f32 * 0.38;
    [
        (appearance.albedo[0] * (1.0 - blend) + wet_soil[0] * blend).clamp(0.0, 1.0),
        (appearance.albedo[1] * (1.0 - blend) + wet_soil[1] * blend).clamp(0.0, 1.0),
        (appearance.albedo[2] * (1.0 - blend) + wet_soil[2] * blend).clamp(0.0, 1.0),
        1.0,
    ]
}

pub(super) fn grid_tangents(
    points: &[DVec3],
    resolution: usize,
    i: usize,
    j: usize,
) -> (DVec3, DVec3) {
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

pub(super) fn outward_normal(normal: DVec3, position: DVec3) -> DVec3 {
    let normal = normal.normalize_or_zero();
    if normal.dot(position) < 0.0 {
        -normal
    } else {
        normal
    }
}

pub(super) fn mesh_surface_frame(
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
