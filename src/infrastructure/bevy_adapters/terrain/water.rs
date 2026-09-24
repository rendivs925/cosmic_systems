//! Planetary water surface presentation.
//!
//! Terrain patches that contain ocean contribute a sea-level sphere cap with a
//! glossy, wave-perturbed material. Water is presentation only: it never feeds
//! collision, radar altitude, or any authoritative terrain sample. The material
//! is shared by every patch, and its single uniform carries the presentation
//! clock and colour/opacity ramp.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// Fragment shader for the water surface.
pub const WATER_SHADER: &str = "shaders/water.wgsl";

/// Shared uniform for every water patch. Must match `WaterParams` in
/// `assets/shaders/water.wgsl` exactly.
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct WaterParams {
    /// Presentation clock in seconds; advances the wave phase only.
    pub time_s: f32,
    /// Spatial frequency of the two crossed swell trains, in 1/m.
    pub wave_scale: f32,
    /// Swell normal-perturbation strength.
    pub wave_strength: f32,
    pub shallow_color: Vec4,
    pub deep_color: Vec4,
    pub opacity_shallow: f32,
    pub opacity_deep: f32,
    /// Breaking-wave foam colour blended onto the shoreline band.
    pub foam_color: Vec4,
    /// Depth of the foam band, as the normalized vertex-depth channel value.
    pub foam_depth_normalized: f32,
    /// Peak foam coverage at the waterline.
    pub foam_strength: f32,
    /// Spatial frequency of the fine ripple train, in 1/m.
    pub ripple_scale: f32,
    /// Fine-ripple normal-perturbation strength.
    pub ripple_strength: f32,
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            time_s: 0.0,
            wave_scale: 0.06,
            wave_strength: 0.35,
            // Shallow coastal water is greener and more transparent; deep ocean
            // is darker blue and near-opaque.
            shallow_color: Vec4::new(0.11, 0.42, 0.45, 1.0),
            deep_color: Vec4::new(0.012, 0.07, 0.16, 1.0),
            opacity_shallow: 0.45,
            opacity_deep: 0.92,
            foam_color: Vec4::new(0.82, 0.88, 0.9, 1.0),
            foam_depth_normalized: 0.004,
            foam_strength: 0.7,
            ripple_scale: 0.6,
            ripple_strength: 0.12,
        }
    }
}

impl WaterParams {
    /// Parameter set for narrow inland channels. Rivers carry the drainage
    /// strength in the vertex-depth channel, so the ramp stays shallow and
    /// opaque, the ripple trains are fine and local, and no breaking-wave foam
    /// is drawn away from the ocean.
    pub fn river() -> Self {
        Self {
            shallow_color: Vec4::new(0.07, 0.20, 0.19, 1.0),
            deep_color: Vec4::new(0.02, 0.07, 0.10, 1.0),
            opacity_shallow: 0.5,
            opacity_deep: 0.88,
            foam_strength: 0.0,
            wave_scale: 0.5,
            wave_strength: 0.18,
            ripple_scale: 2.5,
            ripple_strength: 0.1,
            ..Default::default()
        }
    }
}

/// Material extension blending the depth-driven water ramp over the shared PBR
/// base. The base `StandardMaterial` supplies blend state and lighting.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct WaterExtension {
    #[uniform(100)]
    pub params: WaterParams,
}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER.into()
    }
}

/// Water material: lit, blended, and shared by every ocean patch.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;
