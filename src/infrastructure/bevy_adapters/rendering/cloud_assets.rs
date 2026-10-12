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

fn volume_image(data: Vec<u8>, res: usize) -> Image {
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
    image.sampler = repeating_sampler();
    image
}

fn map_image(data: Vec<u8>) -> Image {
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
