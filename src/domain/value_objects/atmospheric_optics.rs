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

    /// Rayleigh particulate density relative to sea level at an altitude.
    fn rayleigh_density(&self, altitude_m: f32) -> f32 {
        (-altitude_m.max(0.0) / self.rayleigh_scale_height_m.max(1.0)).exp()
    }

    /// Mie particulate density relative to sea level at an altitude.
    fn mie_density(&self, altitude_m: f32) -> f32 {
        (-altitude_m.max(0.0) / self.mie_scale_height_m.max(1.0)).exp()
    }

    /// Triangular ozone band density at an altitude, in `[0, 1]`.
    fn ozone_density(&self, altitude_m: f32) -> f32 {
        let half_width = (self.ozone_layer_width_m * 0.5).max(1.0);
        (1.0 - ((altitude_m - self.ozone_layer_altitude_m).abs() / half_width)).clamp(0.0, 1.0)
    }

    /// Per-channel extinction coefficient at an altitude, in `1/m`. This is the
    /// coefficient used for aerial perspective transmittance.
    pub fn extinction_per_m(&self, altitude_m: f32) -> [f32; 3] {
        let rayleigh = self.rayleigh_density(altitude_m);
        let mie = self.mie_density(altitude_m);
        let ozone = self.ozone_density(altitude_m);
        let mie_extinction = (self.mie_scattering_per_m + self.mie_absorption_per_m) * mie;
        [
            self.rayleigh_scattering_per_m[0] * rayleigh
                + mie_extinction
                + self.ozone_absorption_per_m[0] * ozone,
            self.rayleigh_scattering_per_m[1] * rayleigh
                + mie_extinction
                + self.ozone_absorption_per_m[1] * ozone,
            self.rayleigh_scattering_per_m[2] * rayleigh
                + mie_extinction
                + self.ozone_absorption_per_m[2] * ozone,
        ]
    }

    /// Zenith optical depth of the whole atmosphere above `altitude_m`, per
    /// RGB channel. Integrates the shared extinction profile, so it covers
    /// Rayleigh, Mie, and the ozone band with their different scale heights.
    pub fn overhead_optical_depth(&self, altitude_m: f32) -> [f32; 3] {
        let top_m = self.thickness_m();
        let start_m = altitude_m.clamp(0.0, top_m);
        // A 64-step midpoint integration is exact enough for an exponential
        // profile and cheap enough to run per frame.
        const STEPS: u32 = 64;
        let step_m = (top_m - start_m) / STEPS as f32;
        let mut tau = [0.0f32; 3];
        if step_m <= 0.0 {
            return tau;
        }
        for step in 0..STEPS {
            let sample_m = start_m + (step as f32 + 0.5) * step_m;
            let extinction = self.extinction_per_m(sample_m);
            for channel in 0..3 {
                tau[channel] += extinction[channel] * step_m;
            }
        }
        tau
    }

    /// Transmittance of the direct solar beam reaching `altitude_m`, per RGB
    /// channel, when the Sun's cosine with the local vertical is `sun_cos`.
    ///
    /// The slant path is treated as plane-parallel with the standard airmass
    /// `1 / cos(zenith)`, clamped to 50 so a Sun on the horizon cannot produce a
    /// non-finite result. Callers gate the beam at the horizon themselves.
    pub fn sun_transmittance(&self, altitude_m: f32, sun_cos: f32) -> [f32; 3] {
        let airmass = 1.0 / sun_cos.clamp(1.0 / 50.0, 1.0);
        let tau = self.overhead_optical_depth(altitude_m);
        [
            (-tau[0] * airmass).exp(),
            (-tau[1] * airmass).exp(),
            (-tau[2] * airmass).exp(),
        ]
    }

    /// Per-channel scattering coefficient at an altitude, in `1/m`. This drives
    /// the airlight added to distant surfaces.
    pub fn scattering_per_m(&self, altitude_m: f32) -> [f32; 3] {
        let rayleigh = self.rayleigh_density(altitude_m);
        let mie = self.mie_density(altitude_m);
        let mie_scatter = self.mie_scattering_per_m * mie;
        [
            self.rayleigh_scattering_per_m[0] * rayleigh + mie_scatter,
            self.rayleigh_scattering_per_m[1] * rayleigh + mie_scatter,
            self.rayleigh_scattering_per_m[2] * rayleigh + mie_scatter,
        ]
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
    fn sun_transmittance_is_unity_above_the_atmosphere() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let top = optics.thickness_m();
        for sun_cos in [1.0, 0.3, 0.05] {
            let t = optics.sun_transmittance(top, sun_cos);
            for channel in t {
                assert!((channel - 1.0).abs() < 1e-6, "vacuum T={channel}");
            }
        }
    }

    #[test]
    fn sun_transmittance_reddens_and_dims_toward_the_horizon() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let zenith = optics.sun_transmittance(0.0, 1.0);
        let low = optics.sun_transmittance(0.0, 0.1);

        // Blue is scattered out of the direct beam most strongly at zenith.
        assert!(zenith[0] > zenith[1] && zenith[1] > zenith[2]);
        assert!(zenith[2] > 0.5 && zenith[2] < 1.0);
        // A low Sun travels through far more air in every channel.
        for channel in 0..3 {
            assert!(low[channel] < zenith[channel]);
            assert!((0.0..=1.0).contains(&low[channel]));
        }
    }

    #[test]
    fn sun_transmittance_rises_with_altitude() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let sea = optics.sun_transmittance(0.0, 0.8);
        let cloud = optics.sun_transmittance(3_000.0, 0.8);
        let cirrus = optics.sun_transmittance(9_000.0, 0.8);
        for channel in 0..3 {
            assert!(cloud[channel] > sea[channel]);
            assert!(cirrus[channel] > cloud[channel]);
        }
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

    #[test]
    fn sea_level_extinction_is_strongest_in_blue() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let extinction = optics.extinction_per_m(0.0);

        assert!(extinction[2] > extinction[1]);
        assert!(extinction[1] > extinction[0]);
    }

    #[test]
    fn extinction_falls_off_with_altitude() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let sea_level = optics.extinction_per_m(0.0);
        let high_altitude = optics.extinction_per_m(50_000.0);

        for channel in 0..3 {
            assert!(high_altitude[channel] < sea_level[channel]);
        }
    }

    #[test]
    fn scattering_never_exceeds_extinction() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        for altitude_m in [0.0, 5_000.0, 25_000.0, 80_000.0] {
            let extinction = optics.extinction_per_m(altitude_m);
            let scattering = optics.scattering_per_m(altitude_m);
            for channel in 0..3 {
                assert!(scattering[channel] <= extinction[channel] + f32::EPSILON);
            }
        }
    }

    #[test]
    fn ozone_band_adds_absorption_at_its_layer_altitude() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let below = optics.extinction_per_m(5_000.0);
        let in_band = optics.extinction_per_m(optics.ozone_layer_altitude_m);

        // Green is the most absorbed ozone channel, so the ratio of green to
        // blue extinction increases inside the band relative to below it.
        let below_ratio = below[1] / below[2];
        let band_ratio = in_band[1] / in_band[2];
        assert!(band_ratio > below_ratio);
    }
}
