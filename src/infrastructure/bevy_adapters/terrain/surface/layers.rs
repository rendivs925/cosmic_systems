//! Layered ground material: continuous layer weights plus deterministic
//! procedural per-layer PBR texture sets.
//!
//! This module is presentation-only. Layer weights derive exclusively from the
//! authoritative `TerrainSource` samples (`height_m`, `slope_deg_at`,
//! `moisture`, `zone_lat`) and are emitted into `PreparedPatchSurface` and GPU
//! asset state; nothing here is read by collision, altitude, or fixed-step
//! physics. Every texture is generated deterministically at startup, so the
//! layered path needs no external texture assets and reproduces identical bytes
//! for a fixed build (AGENTS.md 26, 44, 50).

use super::super::mips::{mip_chain_rgba8, MipFilter};
use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, TerrainPatch,
};
use crate::domain::services::terrain_source::{slope_deg_at, TerrainSource};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Bounded ground-layer count. Five layers keep the per-fragment sampling budget
/// finite while covering the grass / soil / rock / sand / snow spectrum.
pub(crate) const GROUND_LAYER_COUNT: usize = 5;

/// Resolution (texels per side) of each shared procedural layer texture. The set
/// is uploaded once and shared by every patch, so residency does not grow with
/// the visible patch count.
pub(crate) const LAYER_TEXTURE_RES: u32 = 128;

/// Resolution of the per-patch layer-weight map. Weights are low-frequency
/// ecotones, so a small map is sufficient and keeps per-patch memory bounded.
pub(crate) const LAYER_WEIGHT_TEX_RES: u32 = 32;

/// Stored bytes for one layer-weight map including its mip chain. Used for
/// budget telemetry; it is deliberately small relative to the local surface maps.
pub(crate) const LAYER_WEIGHT_MAP_BYTES: u64 =
    LAYER_WEIGHT_TEX_RES as u64 * LAYER_WEIGHT_TEX_RES as u64 * 4 * 4 / 3;

/// Repetitions per metre of a ground-layer texture. About 250 m per tile keeps
/// the medium-scale layer pattern readable without aliasing at close range.
pub(crate) const LAYER_TILING_SCALE: f32 = 0.004;
/// Gain applied to the blended layer tangent-space normal.
pub(crate) const LAYER_NORMAL_STRENGTH: f32 = 0.5;
/// Repetitions per metre of the near-camera detail overlay (higher frequency
/// than the shared micro detail).
pub(crate) const NEAR_DETAIL_SCALE: f32 = 2.2;
/// Gain of the near-camera detail overlay, faded by view distance in the shader.
pub(crate) const NEAR_DETAIL_STRENGTH: f32 = 0.28;

/// Identity of one bounded ground layer. The order is the shader's layer index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroundLayer {
    Grass,
    Soil,
    Rock,
    Sand,
    Snow,
}

/// Data-driven definition of one ground layer: its albedo tint, roughness,
/// normal strength, tiling scale, and deterministic texture seed. The catalog is
/// the single owner of layer identity and PBR parameters.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayerDescriptor {
    pub(crate) layer: GroundLayer,
    /// Linear-space albedo tint, matching the authoritative `surface_appearance`
    /// palette so the layered path does not shift the established look.
    pub(crate) albedo: [f32; 3],
    pub(crate) roughness: f32,
    /// Tangent-space normal gain for this layer's procedural texture.
    pub(crate) normal_strength: f64,
    /// Medium-scale feature frequency used when generating the layer texture.
    pub(crate) feature_scale: f64,
    pub(crate) seed: u64,
}

/// The single bounded default layer catalog. Values are linear reflectance
/// ranges derived from the existing continuous `surface_appearance` law.
pub(crate) const GROUND_LAYERS: [LayerDescriptor; GROUND_LAYER_COUNT] = [
    LayerDescriptor {
        layer: GroundLayer::Grass,
        albedo: [0.10, 0.22, 0.04],
        roughness: 0.88,
        normal_strength: 3.2,
        feature_scale: 1.0,
        seed: 11,
    },
    LayerDescriptor {
        layer: GroundLayer::Soil,
        albedo: [0.20, 0.14, 0.08],
        roughness: 0.92,
        normal_strength: 5.0,
        feature_scale: 1.6,
        seed: 23,
    },
    LayerDescriptor {
        layer: GroundLayer::Rock,
        albedo: [0.20, 0.18, 0.15],
        roughness: 0.80,
        normal_strength: 7.5,
        feature_scale: 2.4,
        seed: 37,
    },
    LayerDescriptor {
        layer: GroundLayer::Sand,
        albedo: [0.36, 0.30, 0.16],
        roughness: 0.78,
        normal_strength: 2.2,
        feature_scale: 1.3,
        seed: 41,
    },
    LayerDescriptor {
        layer: GroundLayer::Snow,
        albedo: [0.78, 0.82, 0.86],
        roughness: 0.48,
        normal_strength: 1.4,
        feature_scale: 0.9,
        seed: 53,
    },
];

/// There must always be five layers in index order; keep it a compile-time fact.
const _: () = assert!(GROUND_LAYERS.len() == GROUND_LAYER_COUNT);

/// Hermite smoothstep over `[edge0, edge1]`. Continuous and used for every layer
/// transition so the blend has no hard biome bands.
fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    if (edge1 - edge0).abs() < f64::EPSILON {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Continuous, normalized layer weights derived only from authoritative terrain
/// samples. The output sums to one and is a pure function of its inputs, so
/// adjacent LODs that share a source sample agree exactly and regeneration is
/// deterministic.
pub(crate) fn layer_weights(
    height_m: f64,
    slope_deg: f64,
    moisture: f64,
    zone_lat: f64,
) -> [f32; GROUND_LAYER_COUNT] {
    let slope = slope_deg.max(0.0);
    let moisture = moisture.clamp(0.0, 1.0);
    let zone_lat = zone_lat.clamp(0.0, 1.0);
    let polar = ((zone_lat - 0.5).abs() * 2.0).clamp(0.0, 1.0);

    // Steep ground exposes rock continuously.
    let rock = smoothstep(30.0, 58.0, slope);
    // Snow line descends from the equator toward the poles. Rock faces shed snow
    // so a steep cliff stays rocky near its base.
    let snow_start = 4_200.0 * (1.0 - polar).powf(1.6);
    let snow_altitude = smoothstep(snow_start, snow_start + 700.0, height_m);
    let snow = snow_altitude * (1.0 - rock * 0.85);
    // Dry, near-sea-level ground is sand; wetter ground is grass, drier is soil.
    let low = 1.0 - smoothstep(2.0, 70.0, height_m.max(0.0));
    let dry = 1.0 - smoothstep(0.18, 0.55, moisture);
    let sand = low * dry * (1.0 - rock) * (1.0 - snow);
    let bare = (1.0 - rock) * (1.0 - snow) * (1.0 - sand);
    let wet = smoothstep(0.28, 0.68, moisture);
    let grass = bare * wet;
    let soil = bare * (1.0 - wet);

    let mut weights = [grass, soil, rock, sand, snow];
    let sum: f64 = weights.iter().sum();
    if !sum.is_finite() || sum <= 1e-9 {
        return [0.0, 1.0, 0.0, 0.0, 0.0];
    }
    let mut normalized = [0.0f32; GROUND_LAYER_COUNT];
    for (output, weight) in normalized.iter_mut().zip(weights.iter_mut()) {
        *output = (*weight / sum) as f32;
    }
    normalized
}

/// Deterministic seamless layer noise. Integer offsets preserve seamlessness
/// across all octaves of the shared periodic fBm.
fn layer_noise(seed: u64, u: f64, v: f64) -> f64 {
    let ox = (seed % 7) as f64;
    let oy = ((seed / 7) % 7) as f64;
    super::detail_fbm(u + ox, v + oy)
}

/// Build the per-layer albedo/roughness RGBA array: rgb is the tiled albedo
/// variation around the layer tint, alpha is the layer roughness.
fn layer_albedo_roughness_layers() -> Vec<Vec<u8>> {
    let res = LAYER_TEXTURE_RES as usize;
    GROUND_LAYERS
        .iter()
        .map(|descriptor| {
            let mut data = vec![0u8; res * res * 4];
            for j in 0..res {
                for i in 0..res {
                    let u = i as f64 / res as f64;
                    let v = j as f64 / res as f64;
                    let noise = layer_noise(descriptor.seed, u, v);
                    // Medium-scale tint variation plus a finer grain. The layer
                    // identity seeds the second octave so each layer's tile is
                    // visibly distinct.
                    let grain_seed = descriptor
                        .seed
                        .wrapping_add(97)
                        .wrapping_add(descriptor.layer as u64);
                    let variation =
                        1.0 + noise * 0.14 + layer_noise(grain_seed, u * 2.0, v * 2.0) * 0.06;
                    let index = (j * res + i) * 4;
                    for channel in 0..3 {
                        data[index + channel] = ((descriptor.albedo[channel] as f64 * variation)
                            .clamp(0.0, 1.0)
                            * 255.0)
                            .round() as u8;
                    }
                    let roughness = (descriptor.roughness as f64 + noise * 0.08).clamp(0.0, 1.0);
                    data[index + 3] = (roughness * 255.0).round() as u8;
                }
            }
            data
        })
        .collect()
}

/// Build the per-layer tangent-space normal RGBA array: rgb is the encoded
/// normal, alpha is unused (kept neutral). The normal comes from the gradient of
/// the same seamless layer height field.
fn layer_normal_layers() -> Vec<Vec<u8>> {
    let res = LAYER_TEXTURE_RES as usize;
    GROUND_LAYERS
        .iter()
        .map(|descriptor| {
            let mut height = vec![0.0f64; res * res];
            for j in 0..res {
                for i in 0..res {
                    let u = i as f64 / res as f64;
                    let v = j as f64 / res as f64;
                    height[j * res + i] =
                        0.5 + 0.5 * layer_noise(descriptor.seed, u, v) * descriptor.feature_scale;
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
                    let dx =
                        (sample(i as i64 + 1, j as i64) - sample(i as i64 - 1, j as i64)) * 0.5;
                    let dy =
                        (sample(i as i64, j as i64 + 1) - sample(i as i64, j as i64 - 1)) * 0.5;
                    let nx = -dx * descriptor.normal_strength;
                    let ny = -dy * descriptor.normal_strength;
                    let length = (nx * nx + ny * ny + 1.0).sqrt();
                    let index = (j * res + i) * 4;
                    data[index] = (((nx / length) * 0.5 + 0.5) * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    data[index + 1] = (((ny / length) * 0.5 + 0.5) * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    data[index + 2] = (((1.0 / length) * 0.5 + 0.5) * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    data[index + 3] = 255;
                }
            }
            data
        })
        .collect()
}

/// Shared, deterministic per-layer PBR texture set. `albedo_roughness` and
/// `normal` are 2D texture arrays whose layer index matches [`GROUND_LAYERS`].
pub(crate) struct LayerTextureSet {
    pub(crate) albedo_roughness: Image,
    pub(crate) normal: Image,
}

/// Build one 2D texture array from per-layer RGBA8 base levels, concatenating
/// each layer's own mip chain in wgpu's `LayerMajor` order.
fn layer_array_image(layers: Vec<Vec<u8>>, filter: MipFilter) -> Image {
    let res = LAYER_TEXTURE_RES;
    let mut data = Vec::new();
    let mut levels = 1u32;
    for base in &layers {
        let (chain, chain_levels) = mip_chain_rgba8(res, res, base, filter);
        data.extend_from_slice(&chain);
        levels = chain_levels;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: res,
            height: res,
            depth_or_array_layers: layers.len() as u32,
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

/// Generate the shared layer PBR texture arrays deterministically. No external
/// asset is required and the bytes are stable for a fixed build.
pub(crate) fn layer_texture_set() -> LayerTextureSet {
    LayerTextureSet {
        albedo_roughness: layer_array_image(layer_albedo_roughness_layers(), MipFilter::Color),
        normal: layer_array_image(layer_normal_layers(), MipFilter::Normal),
    }
}

/// Build the per-patch layer-weight map. RGB carry grass / soil / rock / sand
/// weights; snow is derived in the shader as the remaining unit, so one RGBA8
/// map encodes all five layers. Channels are floored so the stored sum never
/// exceeds one and the derived snow weight is never spuriously negative.
pub(crate) fn build_layer_weight_map(source: &dyn TerrainSource, patch: &TerrainPatch) -> Image {
    let res = LAYER_WEIGHT_TEX_RES as usize;
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let mut stored = vec![0u8; res * res * 4];
    for j in 0..res {
        for i in 0..res {
            let u = u0 + (u1 - u0) * i as f64 / (res - 1).max(1) as f64;
            let v = v0 + (v1 - v0) * j as f64 / (res - 1).max(1) as f64;
            let direction = face_uv_to_direction(patch.face, u, v);
            let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
            let weights = layer_weights(
                source.height_m(latitude_deg, longitude_deg),
                slope_deg_at(source, latitude_deg, longitude_deg),
                source.moisture(latitude_deg, longitude_deg),
                source.zone_lat(latitude_deg),
            );
            let index = (j * res + i) * 4;
            for channel in 0..4 {
                stored[index + channel] = (f64::from(weights[channel]) * 255.0)
                    .floor()
                    .clamp(0.0, 255.0) as u8;
            }
        }
    }
    let (data, levels) = mip_chain_rgba8(
        LAYER_WEIGHT_TEX_RES,
        LAYER_WEIGHT_TEX_RES,
        &stored,
        MipFilter::Color,
    );
    // Centralized budget guard: the per-patch weight map must stay inside the
    // layer budget before it is uploaded.
    assert!(
        data.len() as u64 <= LAYER_WEIGHT_MAP_BYTES,
        "layer weight map must stay within its configured budget"
    );
    let mut image = Image::new_uninit(
        Extent3d {
            width: LAYER_WEIGHT_TEX_RES,
            height: LAYER_WEIGHT_TEX_RES,
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
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_weights_are_normalized_and_bounded() {
        for (height, slope, moisture, zone) in [
            (0.0, 0.0, 0.5, 0.5),
            (300.0, 5.0, 0.8, 0.5),
            (1_500.0, 42.0, 0.3, 0.2),
            (5_500.0, 10.0, 0.5, 0.95),
            (-500.0, 1.0, 0.1, 0.5),
        ] {
            let weights = layer_weights(height, slope, moisture, zone);
            let sum: f32 = weights.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-4,
                "weights must sum to one at {height}/{slope}/{moisture}/{zone}, got {sum}"
            );
            for weight in weights {
                assert!(
                    (0.0..=1.0).contains(&weight),
                    "weights must stay in [0, 1], got {weight}"
                );
            }
        }
    }

    #[test]
    fn slope_transitions_smoothly_toward_rock() {
        let gentle = layer_weights(200.0, 5.0, 0.6, 0.5);
        let steep = layer_weights(200.0, 60.0, 0.6, 0.5);
        assert!(
            steep[GroundLayer::Rock as usize] > gentle[GroundLayer::Rock as usize],
            "steeper ground must expose more rock"
        );
    }

    #[test]
    fn elevation_transitions_smoothly_toward_snow() {
        let low = layer_weights(1_000.0, 5.0, 0.5, 0.5);
        let high = layer_weights(5_000.0, 5.0, 0.5, 0.5);
        assert!(
            high[GroundLayer::Snow as usize] > low[GroundLayer::Snow as usize],
            "higher ground must expose more snow"
        );
    }

    #[test]
    fn layer_weights_change_continuously_without_hard_bands() {
        // Over a fine slope sweep every weight moves smoothly; a hard biome band
        // would show up as a step much larger than the local gradient allows.
        let mut previous = layer_weights(500.0, 0.0, 0.5, 0.5);
        for index in 1..=400 {
            let slope = index as f64 * 2.0;
            let current = layer_weights(500.0, slope, 0.5, 0.5);
            for (a, b) in previous.iter().zip(current.iter()) {
                assert!(
                    (a - b).abs() < 0.12,
                    "layer weight jumped between slope samples: {a} -> {b}"
                );
            }
            previous = current;
        }
        // The same holds across the snow band as elevation rises.
        let mut previous = layer_weights(0.0, 5.0, 0.5, 0.5);
        for index in 1..=300 {
            let height = index as f64 * 20.0;
            let current = layer_weights(height, 5.0, 0.5, 0.5);
            for (a, b) in previous.iter().zip(current.iter()) {
                assert!(
                    (a - b).abs() < 0.12,
                    "layer weight jumped between elevation samples: {a} -> {b}"
                );
            }
            previous = current;
        }
    }

    #[test]
    fn layer_weights_are_deterministic() {
        let first = layer_weights(1_234.0, 33.0, 0.42, 0.61);
        let second = layer_weights(1_234.0, 33.0, 0.42, 0.61);
        assert_eq!(first, second);
    }

    #[test]
    fn layer_weights_are_independent_of_mesh_lod() {
        // The pure function has no LOD input, so two patches sharing a source
        // sample necessarily agree at their shared edge.
        let source = crate::domain::services::terrain_source::ProceduralTerrainSource::new(
            99, 2_000.0, 800.0, 0,
        );
        let (lat, lon) = (12.0, 34.0);
        let weights = layer_weights(
            source.height_m(lat, lon),
            slope_deg_at(&source, lat, lon),
            source.moisture(lat, lon),
            source.zone_lat(lat),
        );
        let repeat = layer_weights(
            source.height_m(lat, lon),
            slope_deg_at(&source, lat, lon),
            source.moisture(lat, lon),
            source.zone_lat(lat),
        );
        assert_eq!(weights, repeat);
    }

    #[test]
    fn layer_texture_set_is_deterministic_and_an_array() {
        let first = layer_texture_set();
        let second = layer_texture_set();
        assert_eq!(first.albedo_roughness.data, second.albedo_roughness.data);
        assert_eq!(first.normal.data, second.normal.data);
        assert_eq!(
            first
                .albedo_roughness
                .texture_descriptor
                .size
                .depth_or_array_layers,
            GROUND_LAYER_COUNT as u32
        );
        assert_eq!(
            first.normal.texture_descriptor.size.depth_or_array_layers,
            GROUND_LAYER_COUNT as u32
        );
        assert_eq!(first.albedo_roughness.width(), LAYER_TEXTURE_RES);
        assert!(first.albedo_roughness.texture_descriptor.mip_level_count > 1);
        assert!(first.normal.texture_descriptor.mip_level_count > 1);
    }

    #[test]
    fn layer_material_cost_and_residency_baseline() {
        // Performance baseline for task 5.4: CPU cost of the per-patch layer
        // weight map and the resident bytes of the per-patch map plus the shared
        // layer texture arrays. Display-free, so it runs anywhere.
        use crate::domain::services::terrain_source::ProceduralTerrainSource;
        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let patch =
            TerrainPatch::for_direction(bevy::math::DVec3::new(0.3, 0.4, 1.0).normalize(), 12);

        let started = std::time::Instant::now();
        let weight_map = build_layer_weight_map(&source, &patch);
        let gen_ms = started.elapsed().as_secs_f64() * 1e3;
        let per_patch_bytes = weight_map.data.as_ref().map_or(0, |data| data.len() as u64);
        assert!(per_patch_bytes <= LAYER_WEIGHT_MAP_BYTES);

        let shared = layer_texture_set();
        let shared_bytes = shared
            .albedo_roughness
            .data
            .as_ref()
            .map_or(0, |d| d.len() as u64)
            + shared.normal.data.as_ref().map_or(0, |d| d.len() as u64);

        println!(
            "layer material baseline: weight_map_res={LAYER_WEIGHT_TEX_RES} \
             per_patch_bytes={per_patch_bytes} budget_bytes={LAYER_WEIGHT_MAP_BYTES} \
             shared_textures_bytes={shared_bytes} weight_map_gen_ms={gen_ms:.2}"
        );
    }

    #[test]
    fn layer_weight_map_is_packed_and_deterministic() {
        use crate::domain::services::terrain_source::ProceduralTerrainSource;
        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let patch =
            TerrainPatch::for_direction(bevy::math::DVec3::new(0.3, 0.4, 1.0).normalize(), 12);
        let first = build_layer_weight_map(&source, &patch);
        let second = build_layer_weight_map(&source, &patch);
        assert_eq!(first.data, second.data);
        assert_eq!(first.width(), LAYER_WEIGHT_TEX_RES);
        assert!(first.texture_descriptor.mip_level_count > 1);
        let data = first.data.as_ref().unwrap();
        for texel in data.as_chunks::<4>().0 {
            let stored_sum: u32 = texel[..4].iter().map(|value| u32::from(*value)).sum();
            assert!(
                stored_sum <= 255,
                "stored layer weights must leave a non-negative snow remainder"
            );
        }
    }

    /// Sample the base level of a generated layer-weight map at a patch-local UV.
    fn sampled_texel(image: &Image, u0: f64, v0: f64, u1: f64, v1: f64, u: f64, v: f64) -> [u8; 4] {
        let res = LAYER_WEIGHT_TEX_RES as usize;
        let i = (((u - u0) / (u1 - u0)).clamp(0.0, 1.0) * (res - 1) as f64).round() as usize;
        let j = (((v - v0) / (v1 - v0)).clamp(0.0, 1.0) * (res - 1) as f64).round() as usize;
        let data = image.data.as_ref().unwrap();
        let index = (j * res + i) * 4;
        [
            data[index],
            data[index + 1],
            data[index + 2],
            data[index + 3],
        ]
    }

    #[test]
    fn adjacent_lod_weight_maps_agree_on_the_shared_edge() {
        use crate::domain::services::cube_sphere::CubeFace;

        let source = crate::domain::services::terrain_source::ProceduralTerrainSource::new(
            99, 2_000.0, 800.0, 0,
        );
        // The coarse patch's east edge is the fine patch's west edge.
        let coarse = TerrainPatch {
            face: CubeFace::PosZ,
            level: 11,
            tile_x: 0,
            tile_y: 0,
        };
        let fine = TerrainPatch {
            face: CubeFace::PosZ,
            level: 12,
            tile_x: 2,
            tile_y: 0,
        };
        let coarse_map = build_layer_weight_map(&source, &coarse);
        let fine_map = build_layer_weight_map(&source, &fine);

        let (cu0, cv0, cu1, cv1) = coarse.uv_bounds();
        let (fu0, fv0, fu1, fv1) = fine.uv_bounds();
        let res = LAYER_WEIGHT_TEX_RES as usize;
        for j in 0..res {
            // A coarse texel on the shared edge lands exactly on fine texel 2j.
            let v = cv0 + (cv1 - cv0) * j as f64 / (res - 1) as f64;
            let u = cu1;
            assert!(
                (u - fu0).abs() < 1e-12,
                "test setup: the patches must share the east/west edge"
            );
            assert_eq!(
                sampled_texel(&coarse_map, cu0, cv0, cu1, cv1, u, v),
                sampled_texel(&fine_map, fu0, fv0, fu1, fv1, u, v),
                "layer weights must not depend on mesh LOD at a shared sample"
            );
        }
    }

    #[test]
    fn layer_generation_never_mutates_the_authoritative_source() {
        use crate::domain::services::terrain_source::ProceduralTerrainSource;
        let source = ProceduralTerrainSource::new(7, 1_500.0, 400.0, 0);
        let patch =
            TerrainPatch::for_direction(bevy::math::DVec3::new(0.3, 0.4, 1.0).normalize(), 12);
        let before = source.height_m(12.0, 34.0);
        let _ = build_layer_weight_map(&source, &patch);
        assert_eq!(
            source.height_m(12.0, 34.0),
            before,
            "material generation reads the source only and writes no simulation state"
        );
    }

    #[test]
    fn ground_layer_catalog_is_bounded_and_ordered() {
        assert_eq!(GROUND_LAYERS.len(), GROUND_LAYER_COUNT);
        assert_eq!(GROUND_LAYERS[0].layer, GroundLayer::Grass);
        assert_eq!(GROUND_LAYERS[4].layer, GroundLayer::Snow);
        let seeds = GROUND_LAYERS.map(|descriptor| descriptor.seed);
        assert!(seeds.windows(2).all(|pair| pair[0] != pair[1]));
    }
}
