//! Authoritative atmospheric-optics parameters for planetary sky scattering.
//!
//! These are the single source of Rayleigh, Mie, and ozone coefficients used by
//! the physically based sky. They are plain physical values with explicit
//! units and deliberately have no Bevy dependency, so they can be validated
//! without a renderer and consumed by any presentation adapter.
//!
//! Rayleigh scattering is wavelength dependent and non-absorbing. Mie
//! scattering is wavelength independent and absorbing. Ozone is wavelength
//! dependent and absorbing only, and is concentrated in an altitude band rather
//! than falling off exponentially from the surface.

/// Optical properties of a planetary atmosphere, in SI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphericOptics {
    /// Mean surface radius of the emitting body, in meters. The atmosphere
    /// starts at this radius.
    pub bottom_radius_m: f32,
    /// Radius at which the atmosphere is treated as ending, in meters.
    pub top_radius_m: f32,
    /// Average surface albedo used for multiscattering, unitless `[0, 1]`.
    pub ground_albedo: [f32; 3],
    /// Exponential density falloff length for Rayleigh particulate, in meters.
    pub rayleigh_scale_height_m: f32,
    /// Rayleigh scattering coefficient per meter, per RGB channel.
    pub rayleigh_scattering_per_m: [f32; 3],
    /// Exponential density falloff length for Mie particulate, in meters.
    pub mie_scale_height_m: f32,
    /// Mie scattering coefficient per meter.
    pub mie_scattering_per_m: f32,
    /// Mie absorption coefficient per meter.
    pub mie_absorption_per_m: f32,
    /// Mie forward-scattering asymmetry in `(-1, 1)`.
    pub mie_asymmetry: f32,
    /// Altitude of the centre of the ozone layer, in meters.
    pub ozone_layer_altitude_m: f32,
    /// Width of the ozone layer, in meters.
    pub ozone_layer_width_m: f32,
    /// Ozone absorption coefficient per meter, per RGB channel.
    pub ozone_absorption_per_m: [f32; 3],
}

impl AtmosphericOptics {
    /// Standard Earth atmosphere optics. Coefficients follow the physically
    /// based atmosphere reference values; the surface radius is supplied by the
    /// catalog so the sky starts exactly at the authoritative terrain radius.
    pub fn earth(surface_radius_m: f32) -> Self {
        const EARTH_ATMOSPHERE_THICKNESS_M: f32 = 100_000.0;
        Self {
            bottom_radius_m: surface_radius_m,
            top_radius_m: surface_radius_m + EARTH_ATMOSPHERE_THICKNESS_M,
            ground_albedo: [0.3, 0.3, 0.3],
            rayleigh_scale_height_m: 8_000.0,
            rayleigh_scattering_per_m: [5.802e-6, 13.558e-6, 33.100e-6],
            mie_scale_height_m: 1_200.0,
            mie_scattering_per_m: 3.996e-6,
            mie_absorption_per_m: 0.444e-6,
            mie_asymmetry: 0.8,
            ozone_layer_altitude_m: 25_000.0,
            ozone_layer_width_m: 30_000.0,
            ozone_absorption_per_m: [0.650e-6, 1.881e-6, 0.085e-6],
        }
    }

    /// The optics for a catalog body, or `None` when the body has no modelled
    /// atmosphere (airless bodies render a vacuum sky).
    pub fn for_body(catalog_name: &str, surface_radius_m: f32) -> Option<Self> {
        match catalog_name {
            "Earth" => Some(Self::earth(surface_radius_m)),
            _ => None,
        }
    }

    /// Total thickness of the modelled atmosphere, in meters.
    pub fn thickness_m(&self) -> f32 {
        (self.top_radius_m - self.bottom_radius_m).max(0.0)
    }

    /// Whether these optics represent any scattering at all.
    pub fn has_atmosphere(&self) -> bool {
        self.thickness_m() > 0.0
            && self
                .rayleigh_scattering_per_m
                .iter()
                .any(|coefficient| *coefficient > 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_optics_start_at_the_supplied_surface_radius() {
        let radius_m = 6_371_000.0;
        let optics = AtmosphericOptics::earth(radius_m);

        assert_eq!(optics.bottom_radius_m, radius_m);
        assert!(optics.top_radius_m > optics.bottom_radius_m);
        assert!(optics.has_atmosphere());
    }

    #[test]
    fn rayleigh_is_strongest_in_blue_and_ozone_absorbs_green_most() {
        let optics = AtmosphericOptics::earth(6_371_000.0);

        assert!(optics.rayleigh_scattering_per_m[2] > optics.rayleigh_scattering_per_m[0]);
        assert!(optics.ozone_absorption_per_m[1] > optics.ozone_absorption_per_m[2]);
    }

    #[test]
    fn ozone_band_sits_inside_the_modelled_atmosphere() {
        let optics = AtmosphericOptics::earth(6_371_000.0);

        assert!(
            optics.ozone_layer_altitude_m + optics.ozone_layer_width_m * 0.5
                <= optics.thickness_m()
        );
        assert!(optics.ozone_layer_altitude_m > 0.0);
    }

    #[test]
    fn airless_bodies_have_no_optics() {
        assert!(AtmosphericOptics::for_body("Moon", 1_737_000.0).is_none());
    }

    #[test]
    fn optics_are_deterministic_for_identical_inputs() {
        assert_eq!(
            AtmosphericOptics::earth(6_371_000.0),
            AtmosphericOptics::earth(6_371_000.0)
        );
    }
}
