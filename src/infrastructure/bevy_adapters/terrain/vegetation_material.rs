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
/// Depth/shadow vertex shader that applies the same wind as [`VEGETATION_SHADER`]
/// so cast shadows track the swaying foliage.
pub const VEGETATION_PREPASS_SHADER: &str = "shaders/vegetation_prepass.wgsl";

/// Peak horizontal sway displacement at the canopy for the procedural
/// fallback wind, in metres.
const WIND_STRENGTH_M: f32 = 0.35;
/// Temporal frequency of the sway for the procedural fallback wind, in radians
/// per second.
const SWAY_FREQUENCY_RAD_S: f32 = 1.6;
/// Spatial frequency of the sway phase, in radians per metre, so adjacent
/// plants do not sway in lockstep.
const WIND_SPATIAL_FREQUENCY_RAD_M: f32 = 0.08;
/// Peak canopy displacement per metre per second of measured surface wind. A
/// 10 m/s breeze on a mid-latitude tree bends the crown by roughly a third of a
/// metre, which is the fallback value below.
const SWAY_PER_MPS: f32 = WIND_STRENGTH_M / 10.0;
/// Lower bound on the sway amplitude, metres. Leaves move in a light breeze.
const MIN_SWAY_M: f32 = 0.10;
/// Upper bound on the sway amplitude, metres. A tree does not translate a metre
/// and a half even in a gale; foliage streamlines and sheds instead.
const MAX_SWAY_M: f32 = 1.0;
/// Sway frequency at zero wind, radians per second.
const SWAY_BASE_FREQUENCY_RAD_S: f32 = 0.9;
/// Additional sway frequency per metre per second of wind.
const SWAY_FREQUENCY_PER_MPS: f32 = 0.07;
/// Bounds on the sway frequency, radians per second. Below the lower bound the
/// sway reads as a slow lean rather than a flutter; above the upper bound it
/// strobes at typical frame rates.
const MIN_SWAY_FREQUENCY_RAD_S: f32 = 0.8;
const MAX_SWAY_FREQUENCY_RAD_S: f32 = 3.2;

/// Peak horizontal canopy displacement for a measured surface wind speed.
///
/// Linear in the wind speed and clamped at both ends. Surface wind is what the
/// foliage feels; the cloud deck is advected by the mid-tropospheric wind, so
/// only the direction is shared between them, not the magnitude.
pub fn sway_strength_for_surface_wind(speed_mps: f32) -> f32 {
    (speed_mps * SWAY_PER_MPS).clamp(MIN_SWAY_M, MAX_SWAY_M)
}

/// Temporal sway frequency for a measured surface wind speed, in radians per
/// second. Faster wind makes foliage oscillate faster as well as further.
pub fn sway_frequency_for_surface_wind(speed_mps: f32) -> f32 {
    (SWAY_BASE_FREQUENCY_RAD_S + speed_mps * SWAY_FREQUENCY_PER_MPS)
        .clamp(MIN_SWAY_FREQUENCY_RAD_S, MAX_SWAY_FREQUENCY_RAD_S)
}

/// Maximum diffuse transmission (translucency) applied at foliage weight 1.
/// Thin leaf and grass cards transmit backlight, while the per-vertex weight
/// scales this to zero for trunks, branches, and rocks. Above ~0.5 the lit side
/// reads darker than the backlit side, which is the intended leaf look.
pub const VEGETATION_TRANSLUCENCY: f32 = 0.55;
/// Effective scattering thickness for the translucency lobe, in metres. The
/// backlight lobe is offset along the surface normal by this distance.
pub const VEGETATION_THICKNESS_M: f32 = 0.12;

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

    fn prepass_vertex_shader() -> ShaderRef {
        VEGETATION_PREPASS_SHADER.into()
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

/// Extract the field names of the first `VegetationParams` struct in a shader.
#[cfg(test)]
fn vegetation_params_fields(shader: &str) -> Vec<String> {
    let start = shader
        .find("struct VegetationParams {")
        .expect("vegetation shader must declare VegetationParams");
    let body = &shader[start..];
    let end = body.find('}').expect("VegetationParams struct must close");
    body[..end]
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
        .collect()
}

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

    /// The fallback wind speed must reproduce the tuned default amplitude and
    /// rate, so a missing climatology package changes nothing visually.
    #[test]
    fn fallback_wind_speed_reproduces_the_default_sway() {
        assert!((sway_strength_for_surface_wind(10.0) - WIND_STRENGTH_M).abs() < 1e-6);
        assert!((sway_frequency_for_surface_wind(10.0) - SWAY_FREQUENCY_RAD_S).abs() < 1e-6);
    }

    /// Stronger surface wind bends foliage further and faster, and both are
    /// bounded so a gale or a calm cannot produce a silly displacement.
    #[test]
    fn sway_follows_surface_wind_and_is_bounded() {
        let calm = 0.0;
        let breeze = 10.0;
        let gale = 60.0;
        assert!(sway_strength_for_surface_wind(calm) <= sway_strength_for_surface_wind(breeze));
        assert!(sway_strength_for_surface_wind(breeze) < sway_strength_for_surface_wind(gale));
        assert!(sway_frequency_for_surface_wind(calm) <= sway_frequency_for_surface_wind(breeze));
        assert!(sway_frequency_for_surface_wind(breeze) < sway_frequency_for_surface_wind(gale));
        for speed in [0.0f32, 5.0, 10.0, 35.0, 200.0] {
            let strength = sway_strength_for_surface_wind(speed);
            let frequency = sway_frequency_for_surface_wind(speed);
            assert!((MIN_SWAY_M..=MAX_SWAY_M).contains(&strength));
            assert!((MIN_SWAY_FREQUENCY_RAD_S..=MAX_SWAY_FREQUENCY_RAD_S).contains(&frequency));
        }
    }

    #[test]
    fn translucency_constants_are_in_range() {
        assert!(
            (0.0..=1.0).contains(&VEGETATION_TRANSLUCENCY),
            "translucency must be a normalized weight"
        );
        assert!(
            VEGETATION_THICKNESS_M > 0.0 && VEGETATION_THICKNESS_M.is_finite(),
            "translucency lobe thickness must be a positive finite length"
        );
    }

    #[test]
    fn wgsl_vegetation_params_match_the_rust_field_order() {
        for source in [
            include_str!("../../../../assets/shaders/vegetation.wgsl"),
            include_str!("../../../../assets/shaders/vegetation_prepass.wgsl"),
        ] {
            assert_eq!(
                vegetation_params_fields(source),
                VEGETATION_PARAM_FIELDS
                    .iter()
                    .map(|name| name.to_string())
                    .collect::<Vec<_>>(),
                "VegetationParams field order drifted from a vegetation shader"
            );
        }
    }

    /// The depth/shadow vertex pass must apply the identical wind displacement as
    /// the colour pass, or a tree's cast shadow would lag its swaying canopy.
    #[test]
    fn prepass_applies_the_same_wind_as_the_colour_pass() {
        let colour = include_str!("../../../../assets/shaders/vegetation.wgsl");
        let prepass = include_str!("../../../../assets/shaders/vegetation_prepass.wgsl");
        for line in [
            "let bend = sway_weight * sway_weight;",
            "let sway = sin(phase) * vegetation.wind_strength * bend;",
            "let displaced = vertex.position + vegetation.wind_direction * sway;",
            "let phase = vegetation.time_s * vegetation.sway_frequency",
        ] {
            assert!(colour.contains(line), "colour shader is missing `{line}`");
            assert!(prepass.contains(line), "prepass shader is missing `{line}`");
        }
    }
}
