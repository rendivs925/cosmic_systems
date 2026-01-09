use bevy::prelude::*;

#[derive(Clone, Debug)]
pub struct Rocket {
    pub name: String,
    pub dry_mass_kg: f32,
    pub fuel_mass_kg: f32,
    pub max_thrust_kn: f32,
    pub isp_sea_level: f32,
    pub isp_vacuum: f32,
    pub gimbal_range_deg: f32,
    pub diameter_m: f32,
    pub height_m: f32,
}

impl Rocket {
    pub fn falcon9() -> Self {
        Self {
            name: "Falcon 9".to_string(),
            dry_mass_kg: 22200.0,
            fuel_mass_kg: 120000.0,
            max_thrust_kn: 7607.0,
            isp_sea_level: 282.0,
            isp_vacuum: 311.0,
            gimbal_range_deg: 5.0,
            diameter_m: 3.7,
            height_m: 70.0,
        }
    }

    pub fn total_mass_kg(&self) -> f32 {
        self.dry_mass_kg + self.fuel_mass_kg
    }

    pub fn mass_flow_rate_kg_s(&self, throttle: f32) -> f32 {
        // m_dot = thrust / (Isp * g0)
        let g0 = 9.80665; // Standard gravity
        let isp = self.isp_sea_level; // Use sea level for now
        (self.max_thrust_kn * 1000.0 * throttle) / (isp * g0)
    }
}