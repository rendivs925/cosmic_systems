//! Physically based sky presentation parameters.
//!
//! The sky dome and the sea's mirror reflection evaluate the same
//! single-scatter radiance model (`assets/shaders/sky_radiance.wgsl`), so they
//! share one parameter block: a reflected sky that is a different colour from
//! the sky above it is immediately visible on a calm sea. Every value is
//! derived from the bound body's authoritative [`AtmosphericOptics`] and the
//! shared ephemeris Sun; nothing here feeds flight dynamics.

use crate::domain::services::atmosphere::SEA_LEVEL_PRESSURE_PA;
use crate::domain::value_objects::atmospheric_optics::AtmosphericOptics;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

/// Global scale for the sky radiance. The single-scattering integral is
/// already linear in the same solar irradiance that drives the PBR directional
/// light, so the physical value is `1.0`: the integral alone yields a clear
/// daytime sky (zenith ~1.2e3, horizon ~1.7e4) against a sunlit white surface
/// of ~4.0e4 in the same units. It is kept as a named calibration so an
/// artistic adjustment stays in one place; reducing it darkens the sky without
/// changing the Rayleigh/Mie colour balance.
pub const SKY_RADIANCE_SCALE: f32 = 1.0;

/// Maximum strength of the bounded diffuse multiple-scattering fill.
///
/// The single-scattered sky is too chromatic at altitude and the limb: the long
/// path removes the light that would have been scattered back by the rest of the
/// atmosphere. The fill approximates that diffuse field as an isotropic, gray
/// component and is a convex mix toward the radiance's own luminance, so it is
/// luminance-preserving and bounded by construction. `k_max < 1` keeps a floor
/// of the single-scattered chromaticity. This is an approximation of the diffuse
/// fill's desaturating effect, not a second-order radiative-transfer solve; see
/// `openspec/changes/add-sky-multiple-scattering/design.md`.
pub const SKY_MULTI_SCATTER_MAX: f32 = 0.6;

/// Observer-altitude scale height for the diffuse fill, in meters.
///
/// The local single-scattered source falls with density while the diffuse field
/// from the dense layer below persists, so the relative fill grows with altitude
/// and saturates at [`SKY_MULTI_SCATTER_MAX`].
pub const SKY_MULTI_SCATTER_SCALE_HEIGHT_M: f32 = 20_000.0;

/// Uniform parameters for the sky radiance model. Field order and types must
/// match the `SkyParams` WGSL struct in `assets/shaders/sky_radiance.wgsl`
/// exactly.
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct SkyParams {
    pub planet_center: Vec3,
    pub bottom_radius: f32,
    pub top_radius: f32,
    pub rayleigh_scale_height: f32,
    pub mie_scale_height: f32,
    pub mie_asymmetry: f32,
    pub rayleigh_scattering: Vec3,
    pub mie_scattering: f32,
    pub mie_absorption: f32,
    pub ozone_layer_altitude: f32,
    pub ozone_layer_width: f32,
    pub ozone_absorption: Vec3,
    pub sun_direction: Vec3,
    pub sun_irradiance: Vec3,
    pub ground_albedo: Vec3,
    pub sky_strength: f32,
    pub multi_scatter_max: f32,
    pub multi_scatter_scale_height: f32,
}

impl SkyParams {
    /// Build the shader parameters for a body, or a transparent vacuum sky when
    /// the body has no modelled atmosphere.
    pub fn for_optics(
        optics: Option<AtmosphericOptics>,
        planet_center: Vec3,
        sun_direction: Vec3,
        sun_irradiance: Vec3,
    ) -> Self {
        let Some(optics) = optics else {
            return Self {
                planet_center,
                sun_direction,
                sun_irradiance,
                ..default()
            };
        };
        Self {
            planet_center,
            bottom_radius: optics.bottom_radius_m,
            top_radius: optics.top_radius_m,
            rayleigh_scale_height: optics.rayleigh_scale_height_m,
            mie_scale_height: optics.mie_scale_height_m,
            mie_asymmetry: optics.mie_asymmetry,
            rayleigh_scattering: Vec3::from(optics.rayleigh_scattering_per_m),
            mie_scattering: optics.mie_scattering_per_m,
            mie_absorption: optics.mie_absorption_per_m,
            ozone_layer_altitude: optics.ozone_layer_altitude_m,
            ozone_layer_width: optics.ozone_layer_width_m,
            ozone_absorption: Vec3::from(optics.ozone_absorption_per_m),
            sun_direction,
            sun_irradiance,
            ground_albedo: Vec3::from(optics.ground_albedo),
            sky_strength: SKY_RADIANCE_SCALE,
            multi_scatter_max: SKY_MULTI_SCATTER_MAX,
            multi_scatter_scale_height: SKY_MULTI_SCATTER_SCALE_HEIGHT_M,
        }
    }

    /// Fraction of the radiance replaced by the bounded diffuse fill at an
    /// observer altitude.
    ///
    /// This mirrors `sky_multiple_scatter_fraction` in the shared shader; the
    /// Rust copy exists so the bound and monotonicity are testable without a GPU.
    pub fn multi_scatter_fraction(&self, altitude_m: f32) -> f32 {
        if self.multi_scatter_max <= 0.0 || self.multi_scatter_scale_height <= 0.0 {
            return 0.0;
        }
        let k = self.multi_scatter_max
            * (1.0 - (-altitude_m.max(0.0) / self.multi_scatter_scale_height).exp());
        k.clamp(0.0, 0.999)
    }

    /// Whether these parameters describe a scattering atmosphere. Airless
    /// bodies and the pre-first-frame default report `false`, which lets the
    /// sea skip a reflection it could only get wrong.
    pub fn has_atmosphere(&self) -> bool {
        self.top_radius > self.bottom_radius && self.sky_strength > 0.0
    }

    /// Scale the sea-level molecular scattering coefficients by the measured
    /// surface-pressure ratio. Presentation only: this changes the optical
    /// depth of the sky and the reflected sky, not the flight atmosphere. The
    /// clamp keeps pathological data from producing a black or over-bright sky.
    pub fn apply_measured_surface_pressure(&mut self, pressure_pa: f64) {
        if !pressure_pa.is_finite() || pressure_pa <= 0.0 {
            return;
        }
        let ratio = (pressure_pa / SEA_LEVEL_PRESSURE_PA).clamp(0.5, 1.1) as f32;
        self.rayleigh_scattering *= ratio;
        self.mie_scattering *= ratio;
        self.mie_absorption *= ratio;
    }
}

/// The sky parameters the dome was last drawn with.
///
/// Published by the sky system and consumed by the sea, so the reflection and
/// the sky itself are guaranteed to be the same atmosphere. Absent outside
/// rocket mode, where nothing reflects a sky.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct SkyPresentation(pub SkyParams);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vacuum_bodies_produce_a_transparent_sky() {
        let params = SkyParams::for_optics(None, Vec3::ZERO, Vec3::X, Vec3::splat(127_000.0));

        assert_eq!(params.top_radius, 0.0);
        assert_eq!(params.sky_strength, 0.0);
        assert_eq!(params.rayleigh_scattering, Vec3::ZERO);
        // A vacuum must never be mistaken for reflectable sky.
        assert!(!params.has_atmosphere());
        assert!(!SkyParams::default().has_atmosphere());
    }

    #[test]
    fn earth_optics_flow_into_the_uniform() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let params = SkyParams::for_optics(
            Some(optics),
            Vec3::new(0.0, -6_371_000.0, 0.0),
            Vec3::X,
            Vec3::splat(127_000.0),
        );

        assert_eq!(params.bottom_radius, optics.bottom_radius_m);
        assert_eq!(params.top_radius, optics.top_radius_m);
        assert!(params.rayleigh_scattering.z > params.rayleigh_scattering.x);
        assert_eq!(params.sky_strength, SKY_RADIANCE_SCALE);
        assert!(params.has_atmosphere());
    }

    /// The uniform is written field by field from Bevy's `ShaderType` layout,
    /// so a field inserted on one side and not the other silently shifts every
    /// later value. The dome would keep drawing while the sea reflected a
    /// scrambled atmosphere, which is exactly the kind of break that shows up
    /// as "the water looks slightly wrong" rather than an error.
    #[test]
    fn rust_field_order_matches_the_shader_struct() {
        let shader = include_str!("../../../../assets/shaders/sky_radiance.wgsl");
        let start = shader
            .find("struct SkyParams {")
            .expect("sky radiance shader must declare SkyParams");
        let body = &shader[start..];
        let end = body.find('}').expect("SkyParams struct must close");
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
        assert_eq!(
            fields,
            vec![
                "planet_center",
                "bottom_radius",
                "top_radius",
                "rayleigh_scale_height",
                "mie_scale_height",
                "mie_asymmetry",
                "rayleigh_scattering",
                "mie_scattering",
                "mie_absorption",
                "ozone_layer_altitude",
                "ozone_layer_width",
                "ozone_absorption",
                "sun_direction",
                "sun_irradiance",
                "ground_albedo",
                "sky_strength",
                "multi_scatter_max",
                "multi_scatter_scale_height",
            ],
            "SkyParams field order drifted from assets/shaders/sky_radiance.wgsl"
        );
    }

    #[test]
    fn measured_surface_pressure_scales_only_scattering() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let mut params =
            SkyParams::for_optics(Some(optics), Vec3::ZERO, Vec3::X, Vec3::splat(127_000.0));
        let rayleigh = params.rayleigh_scattering;
        let ground = params.ground_albedo;
        params.apply_measured_surface_pressure(80_000.0);
        let ratio = 80_000.0 / SEA_LEVEL_PRESSURE_PA;
        assert!((params.rayleigh_scattering.x - rayleigh.x * ratio as f32).abs() < 1e-3);
        assert_eq!(params.ground_albedo, ground);
        params.apply_measured_surface_pressure(f64::NAN);
        assert!((params.rayleigh_scattering.x - rayleigh.x * ratio as f32).abs() < 1e-3);
    }

    #[test]
    fn multiple_scattering_fill_grows_with_altitude_and_is_zero_in_vacuum() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let params =
            SkyParams::for_optics(Some(optics), Vec3::ZERO, Vec3::X, Vec3::splat(127_000.0));
        assert_eq!(params.multi_scatter_fraction(0.0), 0.0);
        let mut previous = 0.0;
        for altitude in [1_000.0, 10_000.0, 30_000.0, 70_000.0, 200_000.0] {
            let fraction = params.multi_scatter_fraction(altitude);
            assert!(fraction >= previous, "fill must not decrease with altitude");
            assert!(
                fraction <= SKY_MULTI_SCATTER_MAX,
                "fill must stay bounded, got {fraction}"
            );
            previous = fraction;
        }
        assert!((previous - SKY_MULTI_SCATTER_MAX).abs() < 1e-3);

        let vacuum = SkyParams::for_optics(None, Vec3::ZERO, Vec3::X, Vec3::ZERO);
        assert_eq!(vacuum.multi_scatter_fraction(70_000.0), 0.0);
    }

    /// The fill is a convex mix toward the radiance's own luminance. That is
    /// the analytic energy bound: the result is non-negative, its luminance is
    /// exactly the input's, and every channel lies on the segment between the
    /// input and the luminance-equivalent gray.
    #[test]
    fn multiple_scattering_mix_is_bounded_and_luminance_preserving() {
        let luminance = |value: Vec3| 0.2126 * value.x + 0.7152 * value.y + 0.0722 * value.z;
        for radiance in [
            Vec3::new(0.6, 2.1, 5.6),
            Vec3::new(1600.0, 3030.0, 5426.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(0.0, 0.0, 0.0),
        ] {
            for k in [0.0, 0.1, SKY_MULTI_SCATTER_MAX] {
                let gray = luminance(radiance);
                let mixed = radiance.lerp(Vec3::splat(gray), k);
                assert!(mixed.min_element() >= -1e-6, "fill must not go negative");
                assert!(
                    (luminance(mixed) - gray).abs() <= 1e-3,
                    "fill must preserve luminance"
                );
                for channel in 0..3 {
                    let low = radiance[channel].min(gray) - 1e-3;
                    let high = radiance[channel].max(gray) + 1e-3;
                    assert!(
                        (low..=high).contains(&mixed[channel]),
                        "channel {channel} left the convex hull: {}",
                        mixed[channel]
                    );
                }
            }
        }
    }
}
