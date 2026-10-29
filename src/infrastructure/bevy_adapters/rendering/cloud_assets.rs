//! Startup-generated cloud noise textures (cloud engine spec section 7).
//!
//! The volumetric cloud renderer samples a low-frequency Perlin-Worley base
//! volume, a high-frequency Worley detail volume, and a 2D weather map. All
//! three are generated once from the deterministic [`crate::domain::services::cloud_noise`]
//! functions and stored here as GPU-ready `Image` assets. Nothing is regenerated
//! per frame.

use crate::domain::services::cloud_noise::{
    base_noise_volume, detail_noise_volume, weather_map, BASE_NOISE_RES, DETAIL_NOISE_RES,
    WEATHER_MAP_RES,
};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Fixed seed for the shared cloud field. Presentation-only and deterministic;
/// every run produces the same clouds.
pub const CLOUD_NOISE_SEED: u64 = 0x5EED_C10D;

/// Handles to the three cloud noise textures, generated once at rocket-mode
/// startup.
#[derive(Resource, Debug, Clone)]
pub struct CloudNoiseAssets {
    /// Perlin-Worley base density volume (RGBA8, `BASE_NOISE_RES^3`).
    pub base_noise: Handle<Image>,
    /// Worley detail volume for edge erosion (RGBA8, `DETAIL_NOISE_RES^3`).
    pub detail_noise: Handle<Image>,
    /// 2D weather map (RGBA8, `WEATHER_MAP_RES^2`).
    pub weather_map: Handle<Image>,
}

impl CloudNoiseAssets {
    /// Build and register the three textures from `seed`.
    pub fn generate(images: &mut Assets<Image>, seed: u64) -> Self {
        Self {
            base_noise: images.add(volume_image(base_noise_volume(seed), BASE_NOISE_RES)),
            detail_noise: images.add(volume_image(detail_noise_volume(seed), DETAIL_NOISE_RES)),
            weather_map: images.add(map_image(weather_map(seed))),
        }
    }
}

/// Sampling shared by all three textures: linear filtering and repeat wrapping,
/// so the fields tile seamlessly across the planet.
fn repeating_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::default()
    })
}

/// Wrap coordinates in `[0, res)` so a 3D box filter reads the periodic field
/// across the volume's own tile seams.
fn wrap(index: usize, res: usize) -> usize {
    index % res.max(1)
}

/// Box-filtered RGBA8 volume downsampled by two per axis, wrapping at the seams
/// so the mip chain keeps the field's periodicity.
fn downsample_volume(src: &[u8], res: usize, next: usize) -> Vec<u8> {
    let mut out = vec![0u8; next * next * next * 4];
    let span = (res / next.max(1)).max(1);
    for z in 0..next {
        for y in 0..next {
            for x in 0..next {
                let mut sum = [0u32; 4];
                let mut count = 0u32;
                for dz in 0..span {
                    for dy in 0..span {
                        for dx in 0..span {
                            let sx = wrap(x * span + dx, res);
                            let sy = wrap(y * span + dy, res);
                            let sz = wrap(z * span + dz, res);
                            let index = ((sz * res + sy) * res + sx) * 4;
                            for channel in 0..4 {
                                sum[channel] += u32::from(src[index + channel]);
                            }
                            count += 1;
                        }
                    }
                }
                let index = ((z * next + y) * next + x) * 4;
                for channel in 0..4 {
                    out[index + channel] = ((sum[channel] + count / 2) / count.max(1)) as u8;
                }
            }
        }
    }
    out
}

/// Box-filtered RGBA8 map downsampled by two per axis with wrapping.
fn downsample_map(src: &[u8], res: usize, next: usize) -> Vec<u8> {
    let mut out = vec![0u8; next * next * 4];
    let span = (res / next.max(1)).max(1);
    for y in 0..next {
        for x in 0..next {
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for dy in 0..span {
                for dx in 0..span {
                    let sx = wrap(x * span + dx, res);
                    let sy = wrap(y * span + dy, res);
                    let index = (sy * res + sx) * 4;
                    for channel in 0..4 {
                        sum[channel] += u32::from(src[index + channel]);
                    }
                    count += 1;
                }
            }
            let index = (y * next + x) * 4;
            for channel in 0..4 {
                out[index + channel] = ((sum[channel] + count / 2) / count.max(1)) as u8;
            }
        }
    }
    out
}

/// Number of mip levels down to 1 texel.
fn mip_level_count(res: usize) -> u32 {
    (usize::BITS - res.max(1).leading_zeros()).max(1)
}

/// Append every mip level to `data`. The sampler already asks for linear mip
/// filtering; without a chain a minified 3D volume aliases into per-pixel grit
/// at distance and shimmers as the camera moves.
fn with_mip_chain(
    data: &[u8],
    res: usize,
    downsample: fn(&[u8], usize, usize) -> Vec<u8>,
) -> Vec<u8> {
    let mut chain = Vec::with_capacity(data.len() * 2);
    chain.extend_from_slice(data);
    let mut level = data.to_vec();
    let mut level_res = res;
    while level_res > 1 {
        let next = (level_res / 2).max(1);
        level = downsample(&level, level_res, next);
        chain.extend_from_slice(&level);
        level_res = next;
    }
    chain
}

fn volume_image(data: Vec<u8>, res: usize) -> Image {
    // `Image::new` validates the texel data against the base level, so build
    // with that level and then install the full chain.
    let chain = with_mip_chain(&data, res, downsample_volume);
    let mut image = Image::new(
        Extent3d {
            width: res as u32,
            height: res as u32,
            depth_or_array_layers: res as u32,
        },
        TextureDimension::D3,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(chain);
    image.texture_descriptor.mip_level_count = mip_level_count(res);
    image.sampler = repeating_sampler();
    image
}

fn map_image(data: Vec<u8>) -> Image {
    let chain = with_mip_chain(&data, WEATHER_MAP_RES, downsample_map);
    let mut image = Image::new(
        Extent3d {
            width: WEATHER_MAP_RES as u32,
            height: WEATHER_MAP_RES as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(chain);
    image.texture_descriptor.mip_level_count = mip_level_count(WEATHER_MAP_RES);
    image.sampler = repeating_sampler();
    image
}

/// Generate the cloud noise textures once and expose them as a resource.
pub fn setup_cloud_noise_assets(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(CloudNoiseAssets::generate(&mut images, CLOUD_NOISE_SEED));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_volumes_are_3d_and_correctly_sized() {
        let mut images = Assets::<Image>::default();
        let assets = CloudNoiseAssets::generate(&mut images, 1);

        let base = images.get(&assets.base_noise).expect("base volume");
        assert_eq!(base.texture_descriptor.dimension, TextureDimension::D3);
        assert_eq!(base.texture_descriptor.size.width, BASE_NOISE_RES as u32);
        assert_eq!(
            base.texture_descriptor.size.depth_or_array_layers,
            BASE_NOISE_RES as u32
        );

        let detail = images.get(&assets.detail_noise).expect("detail volume");
        assert_eq!(detail.texture_descriptor.dimension, TextureDimension::D3);
        assert_eq!(
            detail.texture_descriptor.size.depth_or_array_layers,
            DETAIL_NOISE_RES as u32
        );

        let weather = images.get(&assets.weather_map).expect("weather map");
        assert_eq!(weather.texture_descriptor.dimension, TextureDimension::D2);
        assert_eq!(
            weather.texture_descriptor.size.width,
            WEATHER_MAP_RES as u32
        );
    }

    #[test]
    fn volumes_carry_a_full_mip_chain() {
        assert_eq!(mip_level_count(1), 1);
        assert_eq!(mip_level_count(2), 2);
        assert_eq!(mip_level_count(BASE_NOISE_RES), 7);
        assert_eq!(mip_level_count(DETAIL_NOISE_RES), 6);
        assert_eq!(mip_level_count(WEATHER_MAP_RES), 9);

        let base = vec![255u8; BASE_NOISE_RES * BASE_NOISE_RES * BASE_NOISE_RES * 4];
        let chained = with_mip_chain(&base, BASE_NOISE_RES, downsample_volume);
        // A 3D chain halves every axis, so the levels sum to 8/7 of the base.
        let expected = base.len() * 8 / 7;
        assert!(
            (chained.len() as f32 - expected as f32).abs() / expected as f32 <= 0.02,
            "3D mip chain is {} bytes, expected about {expected}",
            chained.len()
        );
        // A constant field must stay constant through the box filter.
        assert!(chained.iter().all(|value| *value == 255));
    }

    #[test]
    fn mip_chain_preserves_the_field_mean() {
        let data = base_noise_volume(3);
        let base_len = BASE_NOISE_RES * BASE_NOISE_RES * BASE_NOISE_RES * 4;
        let chained = with_mip_chain(&data, BASE_NOISE_RES, downsample_volume);
        let base_mean: f64 =
            data[..base_len].iter().map(|v| f64::from(*v)).sum::<f64>() / base_len as f64;
        let coarsest = &chained[chained.len() - 4..];
        let coarsest_mean = coarsest.iter().map(|v| f64::from(*v)).sum::<f64>() / 4.0;
        assert!(
            (base_mean - coarsest_mean).abs() < 1.5,
            "1x1x1 mip {coarsest_mean} should match base mean {base_mean}"
        );
    }

    #[test]
    fn weather_map_carries_a_full_2d_mip_chain() {
        let mut images = Assets::<Image>::default();
        let assets = CloudNoiseAssets::generate(&mut images, 1);
        let weather = images.get(&assets.weather_map).expect("weather map");
        assert_eq!(weather.texture_descriptor.mip_level_count, 9);
        // A 2D chain halves both axes, so the levels sum to 4/3 of the base.
        let expected = WEATHER_MAP_RES * WEATHER_MAP_RES * 4 * 4 / 3;
        let data = weather.data.as_ref().expect("weather map texels");
        assert!(
            (data.len() as f32 - expected as f32).abs() / expected as f32 <= 0.02,
            "2D mip chain is {} bytes, expected about {expected}",
            data.len()
        );
    }

    #[test]
    fn noise_textures_repeat_for_seamless_tiling() {
        let mut images = Assets::<Image>::default();
        let assets = CloudNoiseAssets::generate(&mut images, 1);
        let base = images.get(&assets.base_noise).expect("base volume");
        let ImageSampler::Descriptor(sampler) = &base.sampler else {
            panic!("expected an explicit sampler descriptor");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_w, ImageAddressMode::Repeat);
    }
}
