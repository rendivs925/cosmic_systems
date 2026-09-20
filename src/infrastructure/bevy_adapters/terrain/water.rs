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
    /// Spatial frequency of the crossed wave trains, in 1/m.
    pub wave_scale: f32,
    /// Normal-perturbation strength.
    pub wave_strength: f32,
    pub shallow_color: Vec4,
    pub deep_color: Vec4,
    pub opacity_shallow: f32,
    pub opacity_deep: f32,
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
        }
    }
}

/// Material extension blending the depth-driven water ramp over the shared PBR
/// base. The base `StandardMaterial` supplies blend state and lighting.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterExtension {
    #[uniform(100)]
    pub params: WaterParams,
}

impl Default for WaterExtension {
    fn default() -> Self {
        Self {
            params: WaterParams::default(),
        }
    }
}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER.into()
    }
}

/// Water material: lit, blended, and shared by every ocean patch.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;
