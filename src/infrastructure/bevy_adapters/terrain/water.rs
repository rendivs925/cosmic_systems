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

/// Named ocean defaults. Open-ocean swell: a metre-scale primary wave with short
/// chop, a slow absorption ramp, and a teal crest scatter.
const OCEAN_WAVE_HEIGHT_M: f32 = 1.2;
const OCEAN_ABSORPTION: f32 = 2.4;
const OCEAN_SSS_STRENGTH: f32 = 0.35;
const OCEAN_SSS_COLOR: Vec4 = Vec4::new(0.10, 0.36, 0.32, 1.0);
const OCEAN_LANDSCAPE_SHADOW_STRENGTH: f32 = 1.0;
/// Named inland-channel defaults. Shallow and fast: small swell, strong
/// absorption, and a weak green crest scatter mostly shaded by the banks.
const RIVER_WAVE_HEIGHT_M: f32 = 0.12;
const RIVER_ABSORPTION: f32 = 4.0;
const RIVER_SSS_STRENGTH: f32 = 0.18;
const RIVER_SSS_COLOR: Vec4 = Vec4::new(0.06, 0.22, 0.16, 1.0);
const RIVER_LANDSCAPE_SHADOW_STRENGTH: f32 = 0.5;
/// Phase speed, in radians per second, of the flow-directed river ripple. The
/// direction comes per-vertex from the river mesh; this only scales time.
const RIVER_FLOW_SPEED: f32 = 1.4;
/// Maximum and minimum Gerstner wave components accepted by the quality config.
const MAX_WAVE_COMPONENTS: u32 = 4;
const MIN_WAVE_COMPONENTS: u32 = 1;
const DEFAULT_WAVE_COMPONENTS: u32 = MAX_WAVE_COMPONENTS;
const DEFAULT_FOAM_COVERAGE: f32 = 1.0;
const DEFAULT_WATER_SUBDIVISION: u32 = 33;

/// Rust-side mirror of the `WaterParams` field order, checked against the WGSL
/// struct layout so the uniform can never silently drift from the shader.
#[cfg(test)]
const WATER_PARAM_FIELDS: &[&str] = &[
    "time_s",
    "wave_scale",
    "wave_strength",
    "shallow_color",
    "deep_color",
    "opacity_shallow",
    "opacity_deep",
    "foam_color",
    "foam_depth_normalized",
    "foam_strength",
    "ripple_scale",
    "ripple_strength",
    "wave_height_m",
    "absorption",
    "sss_strength",
    "sss_color",
    "landscape_shadow_strength",
    "wave_components",
    "foam_coverage",
    "flow_speed",
];

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
    /// Peak vertical displacement of the primary swell, in meters.
    pub wave_height_m: f32,
    /// Beer-Lambert absorption exponent over the normalized visible depth.
    pub absorption: f32,
    /// Strength of the crest subsurface-scattering tint.
    pub sss_strength: f32,
    /// Colour scattered through wave crests.
    pub sss_color: Vec4,
    /// Fraction of the baked terrain self-shadow applied to water shading, so
    /// terrain beyond the directional-shadow cascades still darkens the sea.
    pub landscape_shadow_strength: f32,
    /// Number of active Gerstner wave components (`1..=4`). Lower counts are a
    /// bounded quality tier for distant or constrained targets.
    pub wave_components: f32,
    /// Global foam coverage multiplier applied to shoreline and crest foam.
    pub foam_coverage: f32,
    /// Phase speed of the flow-directed river ripple, in radians per second.
    /// Zero for the ocean; rivers animate along their per-vertex flow direction.
    pub flow_speed: f32,
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
            wave_height_m: OCEAN_WAVE_HEIGHT_M,
            absorption: OCEAN_ABSORPTION,
            sss_strength: OCEAN_SSS_STRENGTH,
            sss_color: OCEAN_SSS_COLOR,
            landscape_shadow_strength: OCEAN_LANDSCAPE_SHADOW_STRENGTH,
            wave_components: DEFAULT_WAVE_COMPONENTS as f32,
            foam_coverage: DEFAULT_FOAM_COVERAGE,
            flow_speed: 0.0,
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
            wave_height_m: RIVER_WAVE_HEIGHT_M,
            absorption: RIVER_ABSORPTION,
            sss_strength: RIVER_SSS_STRENGTH,
            sss_color: RIVER_SSS_COLOR,
            // Inland channels are narrow and mostly shaded by their banks.
            landscape_shadow_strength: RIVER_LANDSCAPE_SHADOW_STRENGTH,
            flow_speed: RIVER_FLOW_SPEED,
            ..Default::default()
        }
    }
}

/// Quality budget for the water surface. One algorithm, data-driven values: the
/// defaults are the maximum tier, and lower tiers reduce wave components, foam
/// coverage, and displacement subdivision without changing any code path.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct WaterQualityConfig {
    /// Maximum water mesh vertices per side. Caps displacement subdivision for
    /// the ocean cap so a distant or constrained target pays less geometry.
    pub max_subdivision: u32,
    /// Number of active Gerstner wave components, clamped to `1..=4`.
    pub wave_components: u32,
    /// Global foam coverage multiplier.
    pub foam_coverage: f32,
    /// Opt-in screen-space refraction. Disabled by default; when off, or when
    /// the target/platform cannot supply refraction buffers, the alpha-blended
    /// depth path is used instead.
    pub refraction: bool,
}

impl Default for WaterQualityConfig {
    fn default() -> Self {
        Self {
            max_subdivision: DEFAULT_WATER_SUBDIVISION,
            wave_components: DEFAULT_WAVE_COMPONENTS,
            foam_coverage: DEFAULT_FOAM_COVERAGE,
            refraction: false,
        }
    }
}

impl WaterQualityConfig {
    /// The configured wave component count as the shader's float uniform.
    pub fn wave_components_f32(self) -> f32 {
        self.wave_components
            .clamp(MIN_WAVE_COMPONENTS, MAX_WAVE_COMPONENTS) as f32
    }
}

/// Material extension blending the depth-driven water ramp over the shared PBR
/// base. The base `StandardMaterial` supplies blend state and lighting. The
/// terrain-occlusion map is per patch for ocean water, so water can be shaded by
/// the baked landscape self-shadow beyond the directional-shadow cascades.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterExtension {
    #[uniform(100)]
    pub params: WaterParams,
    /// Baked terrain-occlusion map sampled with the patch-local UV1: red is the
    /// self-shadow visibility. Neutral (1, 1) where no patch map is bound.
    #[texture(101)]
    #[sampler(102)]
    pub terrain_occlusion: Handle<Image>,
}

impl WaterExtension {
    /// Build an extension over the given occlusion map. Callers pass a shared
    /// neutral map when no patch-level bake is available.
    pub fn new(params: WaterParams, terrain_occlusion: Handle<Image>) -> Self {
        Self {
            params,
            terrain_occlusion,
        }
    }
}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER.into()
    }

    /// The Gerstner radial displacement needs a custom vertex stage, so the
    /// water material supplies its own vertex shader alongside the fragment one.
    fn vertex_shader() -> ShaderRef {
        WATER_SHADER.into()
    }
}

/// Water material: lit, blended, and shared by every ocean patch.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocean_and_river_params_are_physical_and_distinct() {
        let ocean = WaterParams::default();
        let river = WaterParams::river();
        for params in [ocean, river] {
            assert!(params.wave_height_m.is_finite() && params.wave_height_m >= 0.0);
            assert!(params.absorption.is_finite() && params.absorption >= 0.0);
            assert!(params.sss_strength.is_finite() && (0.0..=1.0).contains(&params.sss_strength));
            assert!((0.0..=1.0).contains(&params.wave_strength));
            assert!((0.0..=1.0).contains(&params.ripple_strength));
            assert!(
                params.landscape_shadow_strength.is_finite()
                    && (0.0..=1.0).contains(&params.landscape_shadow_strength)
            );
        }
        // Ocean swell is larger and survives to greater depth than an inland
        // channel, which stays shallow, small, and strongly absorbing.
        assert!(ocean.wave_height_m > river.wave_height_m);
        assert!(ocean.absorption < river.absorption);
    }

    #[test]
    fn quality_config_is_bounded_and_clamped() {
        let defaults = WaterQualityConfig::default();
        assert_eq!(defaults.wave_components_f32(), 4.0);
        assert!(defaults.max_subdivision >= 2);
        assert!((0.0..=1.0).contains(&defaults.foam_coverage));
        assert!(!defaults.refraction, "refraction is opt-in");

        let low = WaterQualityConfig {
            max_subdivision: 17,
            wave_components: 1,
            foam_coverage: 0.5,
            refraction: true,
        };
        assert_eq!(low.wave_components_f32(), 1.0);
        assert_eq!(low.max_subdivision, 17);
        assert!(low.refraction);

        let clamped_low = WaterQualityConfig {
            wave_components: 0,
            ..defaults
        };
        assert_eq!(clamped_low.wave_components_f32(), 1.0);
        let clamped_high = WaterQualityConfig {
            wave_components: 9,
            ..defaults
        };
        assert_eq!(clamped_high.wave_components_f32(), 4.0);
    }

    #[test]
    fn wgsl_water_params_match_the_rust_field_order() {
        let shader = include_str!("../../../../assets/shaders/water.wgsl");
        let start = shader
            .find("struct WaterParams {")
            .expect("water shader must declare WaterParams");
        let body = &shader[start..];
        let end = body.find('}').expect("WaterParams struct must close");
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
        let expected: Vec<String> = WATER_PARAM_FIELDS
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(
            fields, expected,
            "WaterParams field order drifted from assets/shaders/water.wgsl"
        );
    }
}
