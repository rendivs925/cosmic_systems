use bevy::prelude::*;

#[derive(Resource, Clone, Debug)]
pub struct SolarSystemParameters {
    pub sun_radius_km: f32,
    pub scale_factor: f32,  // For visualization (e.g., 1 AU = 100 units)
    pub time_scale: f32,    // Simulation speed multiplier
    pub show_orbits: bool,
    pub planet_scale: f32,  // Additional scaling for planets to make them visible
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
            scale_factor: 100.0,  // For visualization (e.g., 1 AU = 100 units)
            time_scale: 1.0,      // Simulation speed multiplier
            show_orbits: true,
            planet_scale: 1.0,    // No additional scaling initially
        }
    }

    /// Create parameters optimized for astronomical accuracy with maximum vast distances
    pub fn for_visualization() -> Self {
        Self {
            sun_radius_km: 696342.0,
            scale_factor: 800.0,  // Maximum vast spacing: 1 AU = 800 units for extreme astronomical scale
            time_scale: 10000.0,   // Faster time scale for visible motion at vast distances
            show_orbits: true,
            planet_scale: 35.0,   // Larger planets for visibility at maximum distances
        }
    }

    /// Convert AU to simulation units
    pub fn au_to_units(&self, au: f32) -> f32 {
        au * self.scale_factor
    }

    /// Convert simulation time to days
    pub fn time_to_days(&self, time_seconds: f32) -> f32 {
        time_seconds * self.time_scale / 86400.0
    }
}