use bevy::color::Color;

#[derive(Clone, Debug)]
pub struct Planet {
    pub name: String,
    pub radius_km: f32,
    pub mass_kg: f64,
    pub color: Color,
    pub orbital_distance_au: f32,  // Average distance from Sun in AU
    pub orbital_period_days: f32,
    pub rotation_period_hours: f32,
}

impl Planet {
    pub fn new(
        name: String,
        radius_km: f32,
        mass_kg: f64,
        color: Color,
        orbital_distance_au: f32,
        orbital_period_days: f32,
        rotation_period_hours: f32,
    ) -> Self {
        Self {
            name,
            radius_km,
            mass_kg,
            color,
            orbital_distance_au,
            orbital_period_days,
            rotation_period_hours,
        }
    }

    // Convenience method to create planets with data
    pub fn create_mercury() -> Self {
        Self::new(
            "Mercury".to_string(),
            4879.0,
            3.3011e23,
            Color::srgb(0.5, 0.5, 0.5), // gray rocky
            0.387,
            88.0,
            1407.6,
        )
    }

    pub fn create_venus() -> Self {
        Self::new(
            "Venus".to_string(),
            12104.0,
            4.8675e24,
            Color::srgb(0.9, 0.8, 0.6), // yellowish cloudy
            0.723,
            224.7,
            5832.5,
        )
    }

    pub fn create_earth() -> Self {
        Self::new(
            "Earth".to_string(),
            12756.0,
            5.9724e24,
            Color::srgb(0.2, 0.4, 0.8), // blue with green continents
            1.0,
            365.25,
            24.0,
        )
    }

    pub fn create_mars() -> Self {
        Self::new(
            "Mars".to_string(),
            6792.0,
            6.4171e23,
            Color::srgb(0.8, 0.3, 0.1), // red/orange
            1.524,
            687.0,
            24.6,
        )
    }

    pub fn create_jupiter() -> Self {
        Self::new(
            "Jupiter".to_string(),
            142984.0,
            1.8982e27,
            Color::srgb(0.8, 0.6, 0.4), // orange/brown bands
            5.204,
            4333.0,
            9.9,
        )
    }

    pub fn create_saturn() -> Self {
        Self::new(
            "Saturn".to_string(),
            120536.0,
            5.6834e26,
            Color::srgb(0.9, 0.8, 0.5), // golden rings
            9.539,
            10759.0,
            10.7,
        )
    }

    pub fn create_uranus() -> Self {
        Self::new(
            "Uranus".to_string(),
            51118.0,
            8.6810e25,
            Color::srgb(0.6, 0.8, 0.9), // pale cyan
            19.191,
            30687.0,
            17.2,
        )
    }

    pub fn create_neptune() -> Self {
        Self::new(
            "Neptune".to_string(),
            49528.0,
            1.02413e26,
            Color::srgb(0.3, 0.5, 0.9), // deep azure
            30.061,
            60190.0,
            16.1,
        )
    }

    pub fn create_sun() -> Self {
        Self::new(
            "Sun".to_string(),
            1392684.0,
            1.989e30,
            Color::srgb(1.0, 1.0, 0.9), // bright yellowish-white
            0.0, // Sun doesn't orbit
            0.0,
            609.12, // 25.38 days sidereal rotation
        )
    }
}