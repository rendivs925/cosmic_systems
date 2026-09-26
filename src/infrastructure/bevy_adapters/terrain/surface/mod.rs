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
//!
//! Per-patch surface maps live in [`surface_maps`]; scatter and river meshes
//! live in [`scatter`].

mod layers;
mod scatter;
mod surface_maps;

pub use scatter::{build_river_mesh, build_vegetation_mesh, build_vegetation_mesh_with_land_cover};
pub use surface_maps::build_patch_surfaces;
use surface_maps::SURFACE_TEX_RES;

pub(crate) use layers::{
    build_layer_weight_map, layer_texture_set, LAYER_NORMAL_STRENGTH, LAYER_TILING_SCALE,
    NEAR_DETAIL_SCALE, NEAR_DETAIL_STRENGTH,
};

use super::mips::{mip_chain_rgba8, MipFilter};
use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, PatchGeometry, TerrainPatch,
};
use crate::domain::services::land_cover::LandCoverPackage;
use crate::domain::services::terrain_source::{
    slope_deg_at, surface_appearance, with_river_appearance, TerrainSource,
};
use crate::domain::services::vegetation::VegetationConfig;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::DVec3;
use bevy::prelude::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_mesh::Mesh;

/// Per-patch geometry caps. Full density is reached at local L14; coarser
/// presentation uses fewer plants per square meter within the same mesh cap.
const TREE_COUNT: usize = VegetationConfig::DEFAULT.tree_budget;
/// A bounded carpet of crossed billboards makes close vegetation read as grass
/// without adding entities or unique materials.
const GRASS_CLUMP_COUNT: usize = VegetationConfig::DEFAULT.grass_budget;
const ROCK_COUNT: usize = 28;
const SCATTER_FULL_DENSITY_LEVEL: u32 = VegetationConfig::DEFAULT.full_density_level;
/// Scatter candidates below this land-cover density are dropped entirely; above
/// it they are thinned probabilistically so density falls off smoothly.
const TREE_MIN_DENSITY: f64 = VegetationConfig::DEFAULT.tree_min_density;
const GRASS_MIN_DENSITY: f64 = VegetationConfig::DEFAULT.grass_min_density;
/// Maximum procedurally generated rock bodies in one scree cluster.
const ROCK_MAX_LUMPS: usize = 3;
/// Solid trunk prisms share the single rock tessellation budget.
const TRUNK_SEGMENTS: usize = 6;
/// Procedural rock base-shape tessellation. Rings and segments are bounded so
/// the displaced rock reads as faceted geology without inflating the per-patch
/// merged-mesh reservation beyond the previous boulder footprint by more than a
/// small margin.
const ROCK_SEGMENTS: usize = 8;
const ROCK_RINGS: usize = 5;
/// Crossed double-sided planes per canopy billboard.
const CANOPY_CARD_PLANES: usize = 3;
/// Maximum stacked canopy layers across all species (conifer uses three).
/// Used only to reserve the merged-mesh byte budget.
const CANOPY_CARD_LAYERS: usize = 3;
/// Crossed planes per grass tuft.
const GRASS_CARD_PLANES: usize = 3;
/// Position (12) + normal (12) + vertex colour (16) + UV (8).
const VEGETATION_BYTES_PER_VERTEX: u64 = 48;
const VEGETATION_BYTES_PER_INDEX: u64 = 4;

/// Resolution (texels per side) of the procedural foliage atlas. The atlas is
/// generated deterministically at startup, so no binary texture asset is
/// committed and the shapes stay reproducible for a fixed build.
pub(crate) const VEGETATION_ATLAS_RES: u32 = 128;
/// Atlas quadrants, in `[u0, v0, u1, v1]`. `v0` is the top of the region.
const GRASS_UV: [f32; 4] = [0.0, 0.0, 0.5, 0.5];
const CONIFER_UV: [f32; 4] = [0.5, 0.0, 1.0, 0.5];
const BROADLEAF_UV: [f32; 4] = [0.0, 0.5, 0.5, 1.0];
/// A fully opaque atlas texel for solid geometry (trunks and boulders) that must
/// not be discarded by the foliage material's alpha mask.
const OPAQUE_UV: [f32; 2] = [0.75, 0.75];

/// Minimum drainage strength at which a river ribbon texel is emitted.
const RIVER_MIN_STRENGTH: f64 = 0.28;
/// Lift of the river surface above the sampled terrain, in meters. Small enough
/// to hug the channel, large enough to avoid z-fighting with the ground.
const RIVER_SURFACE_OFFSET_M: f64 = 0.35;
/// Position (12) + normal (12) + UV (8) + vertex colour (16).
const RIVER_BYTES_PER_VERTEX: u64 = 48;
const RIVER_BYTES_PER_INDEX: u64 = 4;

/// Conservative upper bound for a patch's river ribbon at the given core grid
/// resolution. Used by the streaming memory budget so a fine patch that crosses
/// a drainage network reserves its water geometry up front.
pub(crate) fn max_river_mesh_bytes(resolution: u32) -> u64 {
    let res = u64::from(resolution.max(2));
    let vertices = res * res;
    let indices = 6 * (res - 1) * (res - 1);
    vertices * RIVER_BYTES_PER_VERTEX + indices * RIVER_BYTES_PER_INDEX
}
/// Vegetation is deferred until close-range geometry is available; coarser
/// patches retain the global geographic albedo and scalar surface properties.
pub(crate) const VEGETATION_MIN_PATCH_LEVEL: u32 = 12;

/// Local albedo/normal maps begin one level coarser than vegetation. A patch's
/// detail then fades in over three LOD rings instead of appearing on a single
/// hard boundary, which otherwise reads as one detailed block beside flat ones.
pub(crate) const LOCAL_SURFACE_MIN_PATCH_LEVEL: u32 = 11;

/// Whether the layered ground material is selected for this build. Native
/// `dem` builds use the layered splat path; browser / no-`dem` builds keep the
/// single-layer surface appearance fallback. Evaluated at runtime so both paths
/// stay compiled and the fallback is explicit.
pub(crate) const LAYERED_MATERIAL_SUPPORTED: bool = cfg!(feature = "dem");

/// Continuous detail contribution for a patch level. Zero below the map range,
/// ramping to full detail by level 13 so adjacent LODs blend rather than pop.
pub(crate) fn local_detail_weight(patch_level: u32) -> f32 {
    match patch_level {
        0..=10 => 0.0,
        11 => 0.4,
        12 => 0.7,
        _ => 1.0,
    }
}

/// Albedo and normal maps are both RGBA8 textures. The stored size includes the
/// full mip chain, which adds one third over the base level.
pub(crate) const LOCAL_SURFACE_MAP_BYTES: u64 =
    SURFACE_TEX_RES as u64 * SURFACE_TEX_RES as u64 * 8 * 4 / 3;

/// Conservative maximum allocation for one merged vegetation mesh. Streaming
/// reserves it for close patches before worker generation knows their biome.
pub(crate) const MAX_VEGETATION_MESH_BYTES: u64 = {
    let tree_vertices = 2 * (TRUNK_SEGMENTS + 1) + CANOPY_CARD_LAYERS * CANOPY_CARD_PLANES * 4;
    let tree_indices = TRUNK_SEGMENTS * 6 + CANOPY_CARD_LAYERS * CANOPY_CARD_PLANES * 6;
    let boulder_vertices = (ROCK_RINGS + 1) * ROCK_SEGMENTS;
    let boulder_indices = ROCK_RINGS * ROCK_SEGMENTS * 6;
    let rock_vertices = ROCK_COUNT * ROCK_MAX_LUMPS * boulder_vertices;
    let rock_indices = ROCK_COUNT * ROCK_MAX_LUMPS * boulder_indices;
    let grass_vertices = GRASS_CARD_PLANES * 4;
    let grass_indices = GRASS_CARD_PLANES * 6;
    let vertices = TREE_COUNT * tree_vertices + rock_vertices + GRASS_CLUMP_COUNT * grass_vertices;
    let indices = TREE_COUNT * tree_indices + rock_indices + GRASS_CLUMP_COUNT * grass_indices;
    vertices as u64 * VEGETATION_BYTES_PER_VERTEX + indices as u64 * VEGETATION_BYTES_PER_INDEX
};

pub(crate) fn supports_vegetation(patch_level: u32) -> bool {
    patch_level >= VEGETATION_MIN_PATCH_LEVEL
}

pub(crate) fn supports_local_surfaces(patch_level: u32) -> bool {
    patch_level >= LOCAL_SURFACE_MIN_PATCH_LEVEL
}

/// Resolution of the shared tiling micro-detail texture.
const TERRAIN_DETAIL_RES: u32 = 256;
/// Cells across the tile at the base octave. Integral octave frequencies keep
/// every octave seamless when wrapped, so the texture tiles without a seam.
const TERRAIN_DETAIL_PERIOD: i64 = 8;
/// Tangent-space XY gain applied to the detail height gradient.
const TERRAIN_DETAIL_NORMAL_GAIN: f64 = 2.4;

fn detail_hash(ix: i64, iy: i64) -> f64 {
    let h = (ix as u64)
        .wrapping_mul(374_761_393)
        .wrapping_add((iy as u64).wrapping_mul(668_265_263));
    let h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0x00FF_FFFF) as f64 / 0x00FF_FFFF as f64
}

fn detail_noise(x: f64, y: f64, period: i64) -> f64 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = x - ix as f64;
    let fy = y - iy as f64;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let wrap = |value: i64| value.rem_euclid(period);
    let a = detail_hash(wrap(ix), wrap(iy));
    let b = detail_hash(wrap(ix + 1), wrap(iy));
    let c = detail_hash(wrap(ix), wrap(iy + 1));
    let d = detail_hash(wrap(ix + 1), wrap(iy + 1));
    let ab = a + (b - a) * ux;
    let cd = c + (d - c) * ux;
    (ab + (cd - ab) * uy) * 2.0 - 1.0
}

fn detail_fbm(u: f64, v: f64) -> f64 {
    let mut sum = 0.0;
    let mut weight_sum = 0.0;
    let mut amplitude = 0.6;
    let mut period = TERRAIN_DETAIL_PERIOD;
    for _ in 0..3 {
        sum += amplitude * detail_noise(u * period as f64, v * period as f64, period);
        weight_sum += amplitude;
        amplitude *= 0.5;
        period *= 2;
    }
    sum / weight_sum
}

/// Build the shared micro-detail texture sampled triplanar by the terrain
/// shader: RG is a tangent-space normal, B is subtle albedo variation, A is
/// roughness variation. It carries the surface grain that satellite imagery at
/// ten metres per texel cannot resolve. Deterministic and seam-tiled.
pub(crate) fn terrain_detail_texture() -> Image {
    let res = TERRAIN_DETAIL_RES as usize;
    let mut height = vec![0.0f64; res * res];
    for j in 0..res {
        for i in 0..res {
            height[j * res + i] = detail_fbm(i as f64 / res as f64, j as f64 / res as f64);
        }
    }
    let sample = |i: i64, j: i64| {
        let i = i.rem_euclid(res as i64) as usize;
        let j = j.rem_euclid(res as i64) as usize;
        height[j * res + i]
    };
    let mut data = vec![0u8; res * res * 4];
    for j in 0..res {
        for i in 0..res {
            let dx = (sample(i as i64 + 1, j as i64) - sample(i as i64 - 1, j as i64)) * 0.5;
            let dy = (sample(i as i64, j as i64 + 1) - sample(i as i64, j as i64 - 1)) * 0.5;
            let nx = -dx * TERRAIN_DETAIL_NORMAL_GAIN;
            let ny = -dy * TERRAIN_DETAIL_NORMAL_GAIN;
            let length = (nx * nx + ny * ny + 1.0).sqrt();
            let h = height[j * res + i];
            let index = (j * res + i) * 4;
            data[index] = (((nx / length) * 0.5 + 0.5) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            data[index + 1] = (((ny / length) * 0.5 + 0.5) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            data[index + 2] = ((0.5 + h * 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            data[index + 3] = ((0.5 + h * 0.35) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    let (data, levels) = mip_chain_rgba8(res as u32, res as u32, &data, MipFilter::Color);
    let mut image = Image::new_uninit(
        Extent3d {
            width: res as u32,
            height: res as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::default()
    });
    image
}

/// Build the deterministic foliage atlas used by the vegetation material. Every
/// texel is white so the per-vertex colour fully controls hue; only the alpha
/// channel carries shape. A fully opaque quadrant exists for solid trunks and
/// boulders so the material's alpha mask never discards them.
pub(crate) fn vegetation_atlas() -> Image {
    let res = VEGETATION_ATLAS_RES as usize;
    let half = res / 2;
    let mut data = vec![0u8; res * res * 4];

    draw_grass_tuft(&mut data, res, 0, 0, half);
    draw_conifer(&mut data, res, half, 0, half);
    draw_broadleaf_canopy(&mut data, res, 0, half, half);
    fill_region(&mut data, res, half, half, half, 255);

    let mut image = Image::new(
        Extent3d {
            width: res as u32,
            height: res as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 4,
        ..Default::default()
    });
    image
}

fn fill_region(data: &mut [u8], res: usize, ox: usize, oy: usize, size: usize, alpha: u8) {
    for y in oy..(oy + size).min(res) {
        for x in ox..(ox + size).min(res) {
            set_alpha(data, res, x, y, alpha);
        }
    }
}

/// Overwrite alpha only where it is currently lower, so overlapping shapes
/// union instead of erasing each other.
fn raise_alpha(data: &mut [u8], res: usize, x: usize, y: usize, alpha: u8) {
    if x >= res || y >= res {
        return;
    }
    let index = (y * res + x) * 4;
    if alpha > data[index + 3] {
        set_alpha(data, res, x, y, alpha);
    }
}

fn set_alpha(data: &mut [u8], res: usize, x: usize, y: usize, alpha: u8) {
    if x >= res || y >= res {
        return;
    }
    let index = (y * res + x) * 4;
    data[index] = 255;
    data[index + 1] = 255;
    data[index + 2] = 255;
    data[index + 3] = alpha;
}

/// A tuft of tapered blades rising from the bottom edge of the region.
fn draw_grass_tuft(data: &mut [u8], res: usize, ox: usize, oy: usize, size: usize) {
    const BLADES: usize = 9;
    let s = size as f64;
    for blade in 0..BLADES {
        let base_x = s * (0.09 + 0.82 * blade as f64 / (BLADES - 1) as f64);
        let lean = ((blade % 3) as f64 - 1.0) * s * 0.10;
        let height = s * (0.55 + 0.35 * (blade % 4) as f64 / 3.0);
        let steps = (height.ceil() as usize).max(1);
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let y_local = s - 1.0 - height * t;
            let center = base_x + lean * t * t;
            let half_width = s * 0.035 * (1.0 - t) + 0.35;
            let x0 = (center - half_width).floor().max(0.0) as usize;
            let x1 = (center + half_width).ceil().min(s - 1.0) as usize;
            let y = oy + y_local.round().clamp(0.0, s - 1.0) as usize;
            for x in x0..=x1 {
                raise_alpha(data, res, ox + x, y, 255);
            }
        }
    }
}

/// A triangular conifer silhouette over a short trunk.
fn draw_conifer(data: &mut [u8], res: usize, ox: usize, oy: usize, size: usize) {
    let s = size as f64;
    let apex_y = s * 0.02;
    let base_y = s * 0.84;
    let center_x = s * 0.5;
    for y in apex_y as usize..=base_y as usize {
        let t = (y as f64 - apex_y) / (base_y - apex_y);
        let half = s * 0.44 * t + 0.5;
        let x0 = (center_x - half).max(0.0) as usize;
        let x1 = (center_x + half).min(s - 1.0) as usize;
        for x in x0..=x1 {
            raise_alpha(data, res, ox + x, oy + y, 255);
        }
    }
    for y in (s * 0.82) as usize..size {
        for x in (s * 0.45) as usize..=(s * 0.55) as usize {
            raise_alpha(data, res, ox + x, oy + y, 255);
        }
    }
}

/// A rounded broadleaf crown, unioned from overlapping lobes with a few gaps so
/// the silhouette reads as leaves rather than a solid disc.
fn draw_broadleaf_canopy(data: &mut [u8], res: usize, ox: usize, oy: usize, size: usize) {
    let s = size as f64;
    let lobes = [
        (0.38, 0.34, 0.22),
        (0.62, 0.36, 0.23),
        (0.50, 0.52, 0.25),
        (0.30, 0.54, 0.18),
        (0.70, 0.54, 0.18),
        (0.50, 0.30, 0.20),
    ];
    let holes = [(0.44, 0.40, 0.06), (0.58, 0.48, 0.06), (0.38, 0.53, 0.05)];

    for y in 0..size {
        for x in 0..size {
            let lx = x as f64 / s;
            let ly = y as f64 / s;
            let inside = lobes
                .iter()
                .any(|&(cx, cy, r)| ((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt() <= r);
            if inside {
                raise_alpha(data, res, ox + x, oy + y, 255);
            }
        }
    }
    for y in 0..size {
        for x in 0..size {
            let lx = x as f64 / s;
            let ly = y as f64 / s;
            let in_hole = holes
                .iter()
                .any(|&(cx, cy, r)| ((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt() <= r);
            if in_hole {
                set_alpha(data, res, ox + x, oy + y, 0);
            }
        }
    }
    for y in (s * 0.70) as usize..size {
        for x in (s * 0.46) as usize..=(s * 0.54) as usize {
            raise_alpha(data, res, ox + x, oy + y, 255);
        }
    }
}

/// Source-derived patch data built by the streaming worker and consumed once by
/// the render upload path. Keeping it here prevents asset upload from sampling
/// a DEM, triggering erosion, or generating scatter on the presentation thread.
pub(crate) struct PreparedPatchSurface {
    pub vertex_colors: Vec<[f32; 4]>,
    pub roughness: f32,
    pub metallic: f32,
    pub local_surfaces: Option<(Image, Image)>,
    /// Per-patch layer-weight map ([grass, soil, rock, sand]); snow is derived
    /// in the shader. Presentation-only; never read by collision or physics.
    pub layer_weights: Option<Image>,
    pub vegetation: Option<(Mesh, DVec3)>,
    /// River ribbon mesh plus its body-fixed anchor, or `None` when no channel
    /// crosses the patch.
    pub river: Option<(Mesh, DVec3)>,
}

pub(crate) fn prepare_patch_surface(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    geometry: &PatchGeometry,
    radius_m: f64,
    land_cover: Option<&LandCoverPackage>,
) -> PreparedPatchSurface {
    // Global Earth albedo supplies broad geography. Source-derived local maps
    // are generated only once their detail is visible at close range.
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
    // The layered material is selected at runtime from build capability. Browser
    // / no-`dem` builds keep the single-layer surface appearance path unchanged.
    let layer_weights = (LAYERED_MATERIAL_SUPPORTED && supports_local_surfaces(patch.level))
        .then(|| build_layer_weight_map(source, patch));
    let vegetation_anchor = center * (radius_m + height_m);
    let vegetation = supports_vegetation(patch.level)
        .then(|| {
            build_vegetation_mesh_with_land_cover(
                source,
                patch,
                radius_m,
                &vegetation_anchor,
                land_cover,
            )
        })
        .flatten()
        .map(|mesh| (mesh, vegetation_anchor));
    // Rivers only resolve once the drainage network is represented by close
    // geometry, and share the vegetation anchor's local frame.
    let river = supports_local_surfaces(patch.level)
        .then(|| build_river_mesh(source, geometry, &vegetation_anchor))
        .flatten()
        .map(|mesh| (mesh, vegetation_anchor));
    PreparedPatchSurface {
        vertex_colors,
        roughness: appearance.roughness,
        metallic: appearance.metallic,
        local_surfaces,
        layer_weights,
        vegetation,
        river,
    }
}

#[cfg(test)]
mod tests {
    use super::scatter::{
        plan_rock_bodies, rock_acceptance_probability, rock_candidates, scatter_count_for_level,
        MeshAccum, RockBody,
    };
    use super::surface_maps::{
        mesh_surface_frame, papua_tropical_profile, terrain_albedo, SURFACE_TEX_RES,
    };
    use super::*;
    use crate::domain::services::cube_sphere::build_patch_geometry;
    use crate::domain::services::land_cover::{
        LandCoverClass, LandCoverMetadata, LandCoverPackage,
    };
    #[cfg(feature = "dem")]
    use crate::domain::services::planet_factory::PlanetFactory;
    #[cfg(feature = "dem")]
    use crate::domain::services::reference_frames::geodetic_to_terrain_lat_lon;
    #[cfg(feature = "dem")]
    use crate::domain::services::terrain_source::EarthTerrainSource;
    use crate::domain::services::terrain_source::{ElevationBounds, ProceduralTerrainSource};
    #[cfg(feature = "dem")]
    use crate::domain::value_objects::launch_site_coordinates::predefined_sites;
    use bevy_mesh::VertexAttributeValues;

    #[derive(Debug)]
    struct RiverTerrain;

    impl TerrainSource for RiverTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            300.0
        }

        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(300.0, 300.0)
        }

        fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.5
        }

        fn river_strength(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            1.0
        }
    }

    /// A channel confined to the low-latitude half of the patch, so only part of
    /// the grid carries discharge.
    #[derive(Debug)]
    struct BandedRiverTerrain;

    impl TerrainSource for BandedRiverTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            300.0
        }

        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(300.0, 300.0)
        }

        fn river_strength(&self, latitude_deg: f64, _longitude_deg: f64) -> f64 {
            if latitude_deg < 21.0 {
                0.9
            } else {
                0.0
            }
        }
    }

    #[test]
    fn rock_vertices_are_darkened_toward_the_base() {
        let mut accum = MeshAccum::new();
        accum.push_rock(RockBody {
            center: DVec3::ZERO,
            up: DVec3::Y,
            radius: 1.0,
            aspect: DVec3::new(1.0, 0.8, 1.0),
            seed: 1,
            color: [0.4, 0.5, 0.6],
        });
        let mesh = accum.into_mesh();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("rock must carry positions");
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("rock must carry vertex colours");
        };
        let brightness = |color: &[f32; 4]| color[0] + color[1] + color[2];
        let supplied = 0.4 + 0.5 + 0.6;
        let max = colors.iter().map(brightness).fold(0.0f32, f32::max);
        let min = colors.iter().map(brightness).fold(f32::MAX, f32::min);
        assert!(
            max <= supplied + 1e-5,
            "the crown must not exceed the supplied colour"
        );
        assert!(min < max, "the embedded base must be darker than the crown");

        let lowest = positions
            .iter()
            .enumerate()
            .min_by(|a, b| a.1[1].total_cmp(&b.1[1]))
            .map(|(index, _)| index)
            .expect("rock has vertices");
        assert!(
            (brightness(&colors[lowest]) - min).abs() < 1e-6,
            "the lowest vertex must carry the darkest contact colour"
        );
    }

    #[test]
    fn rock_base_is_embedded_below_the_ground_plane() {
        let embed = 0.35;
        let vertical_radius = 0.8;
        let mut accum = MeshAccum::new();
        accum.push_rock(RockBody {
            center: DVec3::new(0.0, vertical_radius - embed, 0.0),
            up: DVec3::Y,
            radius: 1.0,
            aspect: DVec3::new(1.0, 0.8, 1.0),
            seed: 5,
            color: [0.5, 0.5, 0.5],
        });
        let mesh = accum.into_mesh();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("rock must carry positions");
        };
        let lowest = positions
            .iter()
            .map(|position| position[1])
            .fold(f32::MAX, f32::min);
        assert!(
            lowest < 0.0,
            "the displaced base must sink below the ground plane, got {lowest}"
        );
    }

    #[test]
    fn rocks_are_non_spherical_and_vary_in_size_and_aspect() {
        let mut accum = MeshAccum::new();
        accum.push_rock(RockBody {
            center: DVec3::ZERO,
            up: DVec3::Y,
            radius: 1.0,
            aspect: DVec3::splat(1.0),
            seed: 7,
            color: [0.5, 0.5, 0.5],
        });
        let mesh = accum.into_mesh();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("rock must carry positions");
        };
        let radii: Vec<f64> = positions
            .iter()
            .map(|position| {
                ((position[0] * position[0] + position[1] * position[1] + position[2] * position[2])
                    as f64)
                    .sqrt()
            })
            .collect();
        let max = radii.iter().copied().fold(f64::MIN, f64::max);
        let min = radii.iter().copied().fold(f64::MAX, f64::min);
        assert!(
            max - min > 0.05,
            "the displaced base shape must not be a sphere"
        );

        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let bodies = find_rock_bodies(&source);
        assert!(bodies.len() > 1, "expected several rocks on a rocky patch");
        let first_radius = bodies[0].radius;
        let first_aspect = bodies[0].aspect.x / bodies[0].aspect.y;
        assert!(
            bodies
                .iter()
                .any(|body| (body.radius - first_radius).abs() > 1e-6),
            "rock sizes must vary within a patch"
        );
        assert!(
            bodies
                .iter()
                .any(|body| (body.aspect.x / body.aspect.y - first_aspect).abs() > 1e-6),
            "rock aspect ratios must vary within a patch"
        );
    }

    #[test]
    fn steeper_ground_accepts_more_rock_candidates() {
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 12);
        let budget = scatter_count_for_level(ROCK_COUNT, patch.level);
        let gentle = rock_candidates(&patch, budget, |_, _| 2.0);
        let steep = rock_candidates(&patch, budget, |_, _| 40.0);
        assert!(
            steep.len() > gentle.len(),
            "steep ground must accept more rock candidates"
        );
        assert!(steep.len() <= budget, "candidates must respect the budget");
        assert!(
            rock_acceptance_probability(40.0) > rock_acceptance_probability(2.0),
            "acceptance must rise monotonically with slope"
        );
    }

    #[test]
    fn procedural_rock_geometry_is_deterministic() {
        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 12);

        let first = plan_rock_bodies(&source, &patch, 6_371_000.0, &DVec3::ZERO);
        let second = plan_rock_bodies(&source, &patch, 6_371_000.0, &DVec3::ZERO);
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.seed, b.seed);
            assert_eq!(a.color, b.color);
            assert!((a.center - b.center).length() < 1e-9);
            assert!((a.radius - b.radius).abs() < 1e-12);
            assert!((a.aspect - b.aspect).length() < 1e-12);
        }

        let positions = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(values)) => values.clone(),
            _ => panic!("scatter mesh must carry positions"),
        };
        match (
            build_vegetation_mesh(&source, &patch, 6_371_000.0, &DVec3::ZERO),
            build_vegetation_mesh(&source, &patch, 6_371_000.0, &DVec3::ZERO),
        ) {
            (Some(a), Some(b)) => assert_eq!(positions(&a), positions(&b)),
            (None, None) => {}
            _ => panic!("repeat generation must agree"),
        }
    }

    #[test]
    fn rock_scatter_respects_lod_budget_and_patch_cap() {
        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let direction = DVec3::new(0.3, 0.4, 1.0).normalize();
        for level in VEGETATION_MIN_PATCH_LEVEL..=16 {
            let patch = TerrainPatch::for_direction(direction, level);
            let budget = scatter_count_for_level(ROCK_COUNT, level);
            assert!(budget <= ROCK_COUNT);
            let candidates = rock_candidates(&patch, budget, |_, _| 60.0);
            assert!(
                candidates.len() <= budget,
                "L{level} rock candidates exceeded the budget"
            );
            let bodies = plan_rock_bodies(&source, &patch, 6_371_000.0, &DVec3::ZERO);
            assert!(
                bodies.len() <= budget * ROCK_MAX_LUMPS,
                "L{level} rock bodies exceeded the per-patch cap"
            );
        }

        // Rocks are accumulated into the single merged per-patch mesh; there is
        // no per-rock entity or separate draw path.
        let patch = TerrainPatch::for_direction(direction, VEGETATION_MIN_PATCH_LEVEL);
        if let Some(mesh) = build_vegetation_mesh(&source, &patch, 6_371_000.0, &DVec3::ZERO) {
            assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        }
    }

    fn find_rock_bodies(source: &dyn TerrainSource) -> Vec<RockBody> {
        use crate::domain::services::cube_sphere::CubeFace;

        for face in CubeFace::ALL {
            for tile in 0..16u32 {
                for level in [12u32, 13, 14] {
                    let direction = face_uv_to_direction(face, tile as f64 / 16.0, 0.5);
                    let patch = TerrainPatch::for_direction(direction, level);
                    let bodies = plan_rock_bodies(source, &patch, 6_371_000.0, &DVec3::ZERO);
                    if bodies.len() > 3 {
                        return bodies;
                    }
                }
            }
        }
        Vec::new()
    }

    #[test]
    fn river_mesh_follows_drainage_and_is_absent_without_channels() {
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 12);

        let wet = RiverTerrain;
        let geometry = build_patch_geometry(&patch, &wet, 6_371_000.0, 17, 5.0);
        let mesh = build_river_mesh(&wet, &geometry, &DVec3::ZERO)
            .expect("a fully wet patch must produce a river ribbon");
        assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());

        let dry = ProceduralTerrainSource::new(0, 0.0, 0.0, 0);
        let geometry = build_patch_geometry(&patch, &dry, 6_371_000.0, 17, 5.0);
        assert!(
            build_river_mesh(&dry, &geometry, &DVec3::ZERO).is_none(),
            "a patch with no drainage must not allocate a river ribbon"
        );
    }

    #[test]
    fn river_mesh_encodes_a_flow_direction_in_vertex_colour() {
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 8);
        let source = BandedRiverTerrain;
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 17, 5.0);
        let mesh = build_river_mesh(&source, &geometry, &DVec3::ZERO)
            .expect("a partly wet patch must still produce a channel");
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("river mesh must carry vertex colours");
        };
        let mut saw_flow = false;
        for color in colors {
            assert!(
                (0.0..=1.0).contains(&color[1]) && (0.0..=1.0).contains(&color[2]),
                "encoded flow direction must stay in the unit range"
            );
            if (color[1] - 0.5).abs() > 1e-3 || (color[2] - 0.5).abs() > 1e-3 {
                saw_flow = true;
            }
        }
        assert!(
            saw_flow,
            "a channel with a strength gradient must carry a non-neutral flow direction"
        );
    }

    #[test]
    fn river_width_scales_with_discharge_and_stays_deterministic() {
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 8);

        let wide_source = RiverTerrain;
        let wide_geometry = build_patch_geometry(&patch, &wide_source, 6_371_000.0, 17, 5.0);
        let wide = build_river_mesh(&wide_source, &wide_geometry, &DVec3::ZERO)
            .expect("a fully wet patch must produce a river ribbon");

        let banded_source = BandedRiverTerrain;
        let banded_geometry = build_patch_geometry(&patch, &banded_source, 6_371_000.0, 17, 5.0);
        let banded = build_river_mesh(&banded_source, &banded_geometry, &DVec3::ZERO)
            .expect("a partly wet patch must still produce a channel");
        let banded_repeat = build_river_mesh(&banded_source, &banded_geometry, &DVec3::ZERO)
            .expect("deterministic regeneration");

        // Discharge confines the ribbon: the banded channel covers fewer grid
        // cells than the fully wet patch. Each emitted cell is four vertices.
        assert_eq!(wide.count_vertices() % 4, 0);
        assert_eq!(banded.count_vertices() % 4, 0);
        assert!(
            banded.count_vertices() < wide.count_vertices(),
            "a confined channel must be narrower: {} vs {}",
            banded.count_vertices(),
            wide.count_vertices()
        );
        assert_eq!(
            banded.attribute(Mesh::ATTRIBUTE_POSITION),
            banded_repeat.attribute(Mesh::ATTRIBUTE_POSITION),
            "river geometry must be identical on repeated generation"
        );
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
        // The stored data is the complete mip chain (base level plus every
        // halving), so Bevy can sample it without aliasing.
        assert!(
            albedo.texture_descriptor.mip_level_count > 1,
            "local terrain maps must ship a mip chain"
        );
        let expected_bytes: usize = (0..albedo.texture_descriptor.mip_level_count)
            .map(|level| {
                let size = (SURFACE_TEX_RES >> level).max(1) as usize;
                size * size * 4
            })
            .sum();
        assert_eq!(albedo.data.as_ref().unwrap().len(), expected_bytes);
        assert_eq!(
            normal.texture_descriptor.mip_level_count,
            albedo.texture_descriptor.mip_level_count
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
    fn terrain_albedo_preserves_source_biome_color_in_unorm_range() {
        let grass = terrain_albedo(surface_appearance(300.0, 0.6, 0.5, 5.0), Default::default());
        let rock = terrain_albedo(
            surface_appearance(1_500.0, 0.4, 0.5, 60.0),
            Default::default(),
        );

        for channel in grass.into_iter().chain(rock) {
            assert!((0.0..=1.0).contains(&channel));
        }
        assert!(grass[1] > grass[0]);
        assert!(rock[0] > grass[0]);
    }

    #[test]
    fn papua_tropical_profile_is_deterministic_and_source_input_bounded() {
        let coastal_lowland = papua_tropical_profile(-7.2, 139.4, 20.0, 0.9, 4.0);
        let repeat = papua_tropical_profile(-7.2, 139.4, 20.0, 0.9, 4.0);
        let highland = papua_tropical_profile(-7.2, 139.4, 2_000.0, 0.9, 4.0);
        let outside = papua_tropical_profile(35.0, 139.4, 20.0, 0.9, 4.0);

        assert_eq!(coastal_lowland, repeat);
        assert!((0.0..=1.0).contains(&coastal_lowland.tropical_lowland_unit));
        assert!((0.0..=1.0).contains(&coastal_lowland.wet_vegetation_unit));
        assert!(coastal_lowland.wet_vegetation_unit > highland.wet_vegetation_unit);
        assert_eq!(outside, Default::default());
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
            morph_deltas: vec![],
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
        let coarse_surface =
            prepare_patch_surface(&source, &coarse, &coarse_geometry, 6_371_000.0, None);
        let fine_surface = prepare_patch_surface(&source, &fine, &fine_geometry, 6_371_000.0, None);

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
    fn vegetation_atlas_has_transparent_and_opaque_regions() {
        let atlas = vegetation_atlas();
        assert_eq!(atlas.width(), VEGETATION_ATLAS_RES);
        assert_eq!(atlas.height(), VEGETATION_ATLAS_RES);
        let data = atlas.data.as_ref().expect("atlas must own its texels");
        let res = VEGETATION_ATLAS_RES as usize;

        // The foliage quadrants must contain both cut-out transparency and
        // opaque texels, or the alpha mask would render nothing.
        let grass_region = &data[0..(res / 2) * 4 * res / 2];
        let any_gap = grass_region
            .as_chunks::<4>()
            .0
            .iter()
            .any(|texel| texel[3] == 0);
        let any_blade = grass_region
            .as_chunks::<4>()
            .0
            .iter()
            .any(|texel| texel[3] == 255);
        assert!(
            any_gap && any_blade,
            "grass region needs shape, not a solid fill"
        );

        // The solid quadrant used by trunks and boulders must be fully opaque.
        let opaque_index = ((res / 2 + 10) * res + (res / 2 + 10)) * 4;
        assert_eq!(data[opaque_index + 3], 255);
    }

    #[derive(Debug)]
    struct DenseFlatTerrain;

    impl TerrainSource for DenseFlatTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            100.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(100.0, 100.0)
        }
        fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.8
        }
        fn vegetation_density(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            1.0
        }
    }

    fn land_cover_metadata() -> LandCoverMetadata {
        LandCoverMetadata {
            body: "Earth".into(),
            coordinate_frame: "terrain-radial-degrees".into(),
            horizontal_datum: "WGS84".into(),
            source: "test".into(),
            source_resolution_m: 10.0,
            nodata_policy: "fallback".into(),
            source_sha256: "0".repeat(64),
            license: "test".into(),
            conversion_version: 1,
        }
    }

    #[test]
    fn measured_land_cover_suppresses_cover_and_stays_deterministic() {
        let source = DenseFlatTerrain;
        let patch = TerrainPatch::for_direction(DVec3::new(0.2, 0.3, 1.0).normalize(), 12);

        let without =
            build_vegetation_mesh_with_land_cover(&source, &patch, 6_371_000.0, &DVec3::ZERO, None)
                .expect("dense flat ground must grow vegetation");

        let water = LandCoverPackage::from_samples(
            2,
            2,
            -180.0,
            -90.0,
            180.0,
            90.0,
            land_cover_metadata(),
            vec![LandCoverClass::PermanentWater.code(); 4],
        )
        .expect("valid land-cover package");
        let with_a = build_vegetation_mesh_with_land_cover(
            &source,
            &patch,
            6_371_000.0,
            &DVec3::ZERO,
            Some(&water),
        );
        let with_b = build_vegetation_mesh_with_land_cover(
            &source,
            &patch,
            6_371_000.0,
            &DVec3::ZERO,
            Some(&water),
        );

        // Measured water cover removes canopy and ground-cover scatter, leaving
        // only exposed-rock geometry, so the merged mesh shrinks. Repeated
        // generation with the same package is identical.
        let without_vertices = without.count_vertices();
        let with_vertices = with_a.as_ref().map(Mesh::count_vertices).unwrap_or(0);
        assert!(
            with_vertices < without_vertices,
            "land cover must thin cover: {with_vertices} vs {without_vertices}"
        );
        assert_eq!(
            with_vertices,
            with_b.as_ref().map(Mesh::count_vertices).unwrap_or(0)
        );
    }

    #[test]
    fn fully_vegetated_patch_stays_within_the_mesh_budget() {
        let source = DenseFlatTerrain;
        let patch = TerrainPatch::for_direction(DVec3::new(0.2, 0.3, 1.0).normalize(), 14);
        let start = std::time::Instant::now();
        let mesh = build_vegetation_mesh(&source, &patch, 6_371_000.0, &DVec3::ZERO)
            .expect("dense flat ground must grow vegetation");
        let elapsed = start.elapsed();
        let vertices = mesh.count_vertices() as u64;
        let indices = mesh
            .indices()
            .map(|indices| indices.len() as u64)
            .unwrap_or(0);
        let bytes = vertices * VEGETATION_BYTES_PER_VERTEX + indices * VEGETATION_BYTES_PER_INDEX;
        let budget_pct = bytes as f64 / MAX_VEGETATION_MESH_BYTES as f64 * 100.0;
        println!(
            "vegetation mesh telemetry: vertices={vertices} indices={indices} bytes={bytes} \
             budget_bytes={MAX_VEGETATION_MESH_BYTES} budget_used={budget_pct:.1}% \
             gen_time_us={}",
            elapsed.as_micros()
        );
        assert!(
            bytes <= MAX_VEGETATION_MESH_BYTES,
            "vegetated patch used {bytes} bytes over the reserved {MAX_VEGETATION_MESH_BYTES}"
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
        // the merged mesh must carry vertex colors and foliage UVs so it renders
        // opaque where intended and discards correctly elsewhere.
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
                    assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
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
    fn deterministic_vegetation_is_restricted_to_close_range_patches() {
        assert!(!supports_vegetation(VEGETATION_MIN_PATCH_LEVEL - 1));
        assert!(supports_vegetation(VEGETATION_MIN_PATCH_LEVEL));
    }

    #[test]
    fn local_surface_maps_are_restricted_to_close_range_patches() {
        assert!(!supports_local_surfaces(LOCAL_SURFACE_MIN_PATCH_LEVEL - 1));
        assert!(supports_local_surfaces(LOCAL_SURFACE_MIN_PATCH_LEVEL));
        // Vegetation still starts one level finer than local maps.
        assert!(!supports_vegetation(LOCAL_SURFACE_MIN_PATCH_LEVEL));
        assert!(supports_vegetation(VEGETATION_MIN_PATCH_LEVEL));
    }

    #[test]
    fn detail_texture_tiles_seamlessly_with_a_mip_chain() {
        let first = terrain_detail_texture();
        let second = terrain_detail_texture();
        assert_eq!(
            first.data, second.data,
            "detail texture must be deterministic"
        );
        assert_eq!(first.width(), TERRAIN_DETAIL_RES);
        assert_eq!(first.height(), TERRAIN_DETAIL_RES);
        assert!(first.texture_descriptor.mip_level_count > 1);
        let ImageSampler::Descriptor(sampler) = &first.sampler else {
            panic!("detail texture must use explicit sampling");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        // The left and right edge columns must match for a seamless wrap. The
        // wrap is exact because every octave wraps at its own period.
        let data = first.data.as_ref().unwrap();
        let res = TERRAIN_DETAIL_RES as usize;
        for row in 0..res {
            let left = (row * res) * 4;
            let right = (row * res + res - 1) * 4;
            assert!(
                (data[left] as i32 - data[right] as i32).abs() <= 24,
                "row {row} does not wrap"
            );
        }
    }

    #[test]
    fn local_detail_weight_fades_across_three_lod_rings() {
        assert_eq!(local_detail_weight(LOCAL_SURFACE_MIN_PATCH_LEVEL - 1), 0.0);
        let near = local_detail_weight(LOCAL_SURFACE_MIN_PATCH_LEVEL);
        let mid = local_detail_weight(LOCAL_SURFACE_MIN_PATCH_LEVEL + 1);
        let full = local_detail_weight(LOCAL_SURFACE_MIN_PATCH_LEVEL + 2);
        assert!(near > 0.0 && near < mid, "detail must ramp continuously");
        assert!(mid < full, "detail must ramp continuously");
        assert_eq!(full, 1.0);
    }

    #[test]
    fn scatter_reaches_local_density_without_exceeding_patch_caps() {
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL),
            128
        );
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL + 1),
            128
        );
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL + 2),
            128
        );
        assert_eq!(
            scatter_count_for_level(TREE_COUNT, VEGETATION_MIN_PATCH_LEVEL + 3),
            32
        );
        assert_eq!(
            scatter_count_for_level(ROCK_COUNT, VEGETATION_MIN_PATCH_LEVEL + 2),
            28
        );
    }

    /// Exercise the public Earth wrapper and resident data used at startup.
    /// Atlas corners distinguish plant geometry from rocks in the merged mesh.
    #[cfg(feature = "dem")]
    #[test]
    fn earth_launch_site_is_vegetated_at_close_lod() {
        let source = EarthTerrainSource::new();
        let site = predefined_sites::papua_indonesia_coastal_lowland();
        let earth = PlanetFactory::create_by_id(&site.planet_id).unwrap();
        let (latitude_deg, longitude_deg) = geodetic_to_terrain_lat_lon(&site, &earth);
        let density = source.vegetation_density(latitude_deg, longitude_deg);
        assert!(
            density >= TREE_MIN_DENSITY,
            "Papua launch lowland must be vegetated, got density {density}"
        );

        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let direction = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        for level in 12..=14 {
            let patch = TerrainPatch::for_direction(direction, level);
            let origin = direction * 6_371_000.0;
            let mesh = build_vegetation_mesh(&source, &patch, 6_371_000.0, &origin)
                .expect("the launch-site patch must produce a vegetation mesh");
            let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("scatter must carry atlas coordinates");
            };
            let grass_cards = uvs
                .iter()
                .filter(|uv| **uv == [GRASS_UV[0], GRASS_UV[1]])
                .count();
            let tree_cards = uvs
                .iter()
                .filter(|uv| {
                    **uv == [BROADLEAF_UV[0], BROADLEAF_UV[3]]
                        || **uv == [CONIFER_UV[2], CONIFER_UV[1]]
                })
                .count();
            println!("Papua L{level}: cover={density:.3}, grass cards={grass_cards}, tree cards={tree_cards}");
            assert!(grass_cards > 0, "no grass at L{level}");
            assert!(tree_cards > 0, "no trees at L{level}");
        }
    }
}
