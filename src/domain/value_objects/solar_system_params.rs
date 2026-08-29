use bevy::prelude::*;

/// Julian ephemeris date of the J2000.0 epoch. Solar-system propagation uses
/// elapsed days from this epoch as a TDB approximation until a full time-scale
/// service is introduced.
pub const J2000_JULIAN_DATE_TDB: f64 = 2_451_545.0;

#[derive(Resource, Clone, Debug)]
pub struct SolarSystemParameters {
    pub sun_radius_km: f32,
    pub scale_factor: f32, // For visualization (e.g., 1 AU = 100 units)
    time_scale: f32,
    /// Simulation epoch accumulated before the current time-scale segment.
    /// This keeps a warp change from retroactively rescaling elapsed time.
    epoch_offset_days: f64,
    pub show_orbits: bool,
    /// Uniform visual multiplier. Keep this at 1.0 for physically scaled bodies.
    pub planet_scale: f32,
}

impl Default for SolarSystemParameters {
    fn default() -> Self {
        Self::new()
    }
}

impl SolarSystemParameters {
    pub fn new() -> Self {
        Self {
            sun_radius_km: 696342.0,
            scale_factor: 100.0, // For visualization (e.g., 1 AU = 100 units)
            time_scale: 1.0,     // Simulation speed multiplier
            epoch_offset_days: 0.0,
            show_orbits: true,
            planet_scale: 1.0, // No additional scaling initially
        }
    }

    /// Create parameters optimized for astronomical accuracy with maximum vast distances
    pub fn for_visualization() -> Self {
        Self {
            sun_radius_km: 696342.0,
            scale_factor: 75000.0, // 1 AU = 75,000 simulation units
            time_scale: 3000.0,    // Time scale: 3000.0x
            epoch_offset_days: 0.0,
            show_orbits: true,
            // Bodies and orbital distances share this same physical AU scale.
            planet_scale: 1.0,
        }
    }

    /// Convert AU to simulation units
    pub fn au_to_units(&self, au: f32) -> f32 {
        au * self.scale_factor
    }

    /// Convert astronomical units to solar-map display units without reducing
    /// the propagated f64 position to render precision.
    pub fn au_to_units_f64(&self, au: f64) -> f64 {
        au * self.scale_factor as f64
    }

    /// Current solar-map time acceleration relative to the fixed clock.
    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }

    /// Change time acceleration without changing the simulated epoch at
    /// `elapsed_seconds`. The caller supplies the same fixed-clock elapsed time
    /// consumed by the ephemeris evaluator.
    pub fn set_time_scale_at(&mut self, elapsed_seconds: f64, time_scale: f32) {
        let elapsed_seconds = elapsed_seconds.max(0.0);
        let epoch_days = self.time_to_days_f64(elapsed_seconds);
        self.time_scale = time_scale.max(0.0001);
        self.epoch_offset_days = epoch_days - elapsed_seconds * self.time_scale as f64 / 86_400.0;
    }

    /// Convert fixed-clock elapsed time to the phase-continuous solar epoch.
    pub fn time_to_days(&self, time_seconds: f32) -> f32 {
        self.time_to_days_f64(time_seconds as f64) as f32
    }

    /// Convert elapsed presentation time to simulation days without losing the
    /// sub-unit orbital movement of outer-system bodies.
    pub fn time_to_days_f64(&self, time_seconds: f64) -> f64 {
        self.epoch_offset_days + time_seconds.max(0.0) * self.time_scale as f64 / 86_400.0
    }

    /// Convert the phase-continuous solar epoch to Julian date TDB.
    pub fn time_to_julian_date_tdb(&self, time_seconds: f64) -> f64 {
        J2000_JULIAN_DATE_TDB + self.time_to_days_f64(time_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_scale_changes_preserve_the_solar_epoch() {
        let mut solar = SolarSystemParameters::for_visualization();
        let transition_seconds = 12_345.678_9;
        let epoch_before = solar.time_to_days_f64(transition_seconds);

        solar.set_time_scale_at(transition_seconds, 1.0);
        assert!((solar.time_to_days_f64(transition_seconds) - epoch_before).abs() < 1e-12);

        solar.set_time_scale_at(transition_seconds, 10_000.0);
        assert!((solar.time_to_days_f64(transition_seconds) - epoch_before).abs() < 1e-12);

        solar.set_time_scale_at(transition_seconds, 1.0);
        assert!((solar.time_to_days_f64(transition_seconds) - epoch_before).abs() < 1e-12);
    }

    #[test]
    fn time_scale_change_only_affects_future_epoch_progression() {
        let mut solar = SolarSystemParameters::for_visualization();
        let transition_seconds = 300.0;
        let epoch_before = solar.time_to_days_f64(transition_seconds);

        solar.set_time_scale_at(transition_seconds, 10.0);
        let future_seconds = transition_seconds + 60.0;
        let expected_days = epoch_before + 60.0 * 10.0 / 86_400.0;

        assert!((solar.time_to_days_f64(future_seconds) - expected_days).abs() < 1e-12);
    }

    #[test]
    fn solar_epoch_is_j2000_tdb_at_startup() {
        let solar = SolarSystemParameters::for_visualization();

        assert_eq!(solar.time_to_julian_date_tdb(0.0), J2000_JULIAN_DATE_TDB);
    }
}
