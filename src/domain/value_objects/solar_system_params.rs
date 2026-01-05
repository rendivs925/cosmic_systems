use bevy::prelude::*;

#[derive(Resource, Clone, Debug)]
pub struct SolarSystemParameters {
    pub sun_radius_km: f32,
    pub scale_factor: f32, // For visualization (e.g., 1 AU = 100 units)
    pub time_scale: f32,   // Simulation speed multiplier
    pub show_orbits: bool,
    pub planet_scale: f32, // Additional scaling for planets to make them visible
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
            show_orbits: true,
            planet_scale: 1.0, // No additional scaling initially
        }
    }

    /// Create parameters optimized for astronomical accuracy with maximum vast distances
    pub fn for_visualization() -> Self {
        Self {
            sun_radius_km: 696342.0,
            scale_factor: 75000.0, // 1 AU = 75,000 simulation units
            time_scale: 10000.0,   // Faster time for more dynamic default motion
            show_orbits: true,
            // TRUE SIZE RELATIONSHIPS between planets are preserved:
            // - Sun is 109x larger than Earth
            // - Jupiter is 28.6x larger than Earth
            // But we scale ALL planets up together for visibility against orbits
            // (In reality, planets would be invisible dots at true orbital scale)
            planet_scale: 80.0, // Makes planets visible while keeping size ratios accurate
        }
    }

    /// Convert AU to simulation units
    pub fn au_to_units(&self, au: f32) -> f32 {
        au * self.scale_factor
    }

    /// Convert simulation time to days
    pub fn time_to_days(&self, time_seconds: f32) -> f32 {
        let scale = self.time_scale.max(0.0001); // never let simulation stall
        time_seconds * scale / 86400.0
    }
}
