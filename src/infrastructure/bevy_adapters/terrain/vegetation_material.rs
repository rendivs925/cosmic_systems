//! Wind-swayed vegetation presentation material.
//!
//! Vegetation (trees, shrubs, grass) is one merged, alpha-masked mesh per
//! terrain patch. This material extension adds a GPU vertex-wind sway on top of
//! the shared `StandardMaterial` PBR base, so no per-frame CPU work is needed to
//! animate foliage. Wind is presentation only: it never feeds collision, radar
//! altitude, guidance, or any authoritative terrain sample. The sway weight is
//! carried per vertex in the mesh's second UV channel (0 at the base, 1 at the
//! canopy), and the single uniform carries the presentation clock, the wind
//! vector, and the render origin.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// Shader for the wind-swayed vegetation surface.
pub const VEGETATION_SHADER: &str = "shaders/vegetation.wgsl";

/// Peak horizontal sway displacement at the canopy, in metres.
const WIND_STRENGTH_M: f32 = 0.35;
/// Temporal frequency of the sway, in radians per second.
const SWAY_FREQUENCY_RAD_S: f32 = 1.6;
/// Spatial frequency of the sway phase, in radians per metre, so adjacent
/// plants do not sway in lockstep.
const WIND_SPATIAL_FREQUENCY_RAD_M: f32 = 0.08;

/// Shared uniform for the vegetation material. Must match `VegetationParams` in
/// `assets/shaders/vegetation.wgsl` exactly.
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct VegetationParams {
    /// Presentation clock in seconds; advances the sway phase only.
    pub time_s: f32,
    /// Peak horizontal sway displacement at sway weight 1, in metres.
    pub wind_strength: f32,
    /// Temporal frequency of the sway, in radians per second.
    pub sway_frequency: f32,
    /// Spatial frequency of the sway phase, in radians per metre.
    pub wind_spatial_frequency: f32,
    /// Horizontal wind direction in the patch-local body-fixed frame (unit).
    pub wind_direction: Vec3,
    /// Planet centre in the rebased inertial metre frame (the negated render
    /// origin). Subtracting it recovers an origin-invariant body position so the
    /// sway phase stays pinned to the planet when the render origin recentres.
    pub planet_center: Vec3,
}

impl Default for VegetationParams {
    fn default() -> Self {
        Self {
            time_s: 0.0,
            wind_strength: WIND_STRENGTH_M,
            sway_frequency: SWAY_FREQUENCY_RAD_S,
            wind_spatial_frequency: WIND_SPATIAL_FREQUENCY_RAD_M,
            wind_direction: Vec3::new(0.82, 0.0, 0.57).normalize(),
            planet_center: Vec3::ZERO,
        }
    }
}

/// Material extension carrying the wind uniform. The base `StandardMaterial`
/// supplies the alpha-masked atlas sampling and PBR lighting.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct VegetationExtension {
    #[uniform(100)]
    pub params: VegetationParams,
}

impl VegetationExtension {
    pub fn new(params: VegetationParams) -> Self {
        Self { params }
    }
}

impl MaterialExtension for VegetationExtension {
    fn vertex_shader() -> ShaderRef {
        VEGETATION_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        VEGETATION_SHADER.into()
    }
}

/// Vegetation material: alpha-masked, double-sided, and wind-swayed.
pub type VegetationMaterial = ExtendedMaterial<StandardMaterial, VegetationExtension>;

/// Rust-side mirror of the `VegetationParams` field order, checked against the
/// WGSL struct layout so the uniform can never silently drift from the shader.
#[cfg(test)]
const VEGETATION_PARAM_FIELDS: &[&str] = &[
    "time_s",
    "wind_strength",
    "sway_frequency",
    "wind_spatial_frequency",
    "wind_direction",
    "planet_center",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_are_finite_and_normalized() {
        let params = VegetationParams::default();
        assert!(params.time_s == 0.0);
        assert!(params.wind_strength > 0.0 && params.wind_strength.is_finite());
        assert!(params.sway_frequency > 0.0 && params.sway_frequency.is_finite());
        assert!(params.wind_spatial_frequency > 0.0 && params.wind_spatial_frequency.is_finite());
        assert!((params.wind_direction.length() - 1.0).abs() < 1e-5);
        assert_eq!(params.planet_center, Vec3::ZERO);
    }

    #[test]
    fn wgsl_vegetation_params_match_the_rust_field_order() {
        let shader = include_str!("../../../../assets/shaders/vegetation.wgsl");
        let start = shader
            .find("struct VegetationParams {")
            .expect("vegetation shader must declare VegetationParams");
        let body = &shader[start..];
        let end = body.find('}').expect("VegetationParams struct must close");
        let fields: Vec<String> = body[..end]
            .lines()
            .skip(1)
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with("//") {
                    return None;
                }
                line.split_once(':')
                    .map(|(name, _)| name.trim().to_string())
            })
            .collect();
        let expected: Vec<String> = VEGETATION_PARAM_FIELDS
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(
            fields, expected,
            "VegetationParams field order drifted from assets/shaders/vegetation.wgsl"
        );
    }
}
