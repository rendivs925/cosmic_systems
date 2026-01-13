use bevy::prelude::*;

/// Celestial body entity representing planets, moons, and stars
#[derive(Clone, Debug)]
pub struct Planet {
    pub name: String,
    pub radius_km: f32,
    pub mass_kg: f64,
    pub color: Color,
    pub orbital_distance_au: f32, // Average distance from Sun (or parent planet) in AU
    pub orbital_period_days: f32,
    pub rotation_period_hours: f32,
    pub axial_tilt_deg: f32,
    pub parent_entity: Option<String>, // Name of parent body (None for Sun, planet name for moons)
}

#[derive(Debug, Default)]
pub struct PlanetBuilder {
    name: Option<String>,
    radius_km: Option<f32>,
    mass_kg: Option<f64>,
    color: Option<Color>,
    orbital_distance_au: Option<f32>,
    orbital_period_days: Option<f32>,
    rotation_period_hours: Option<f32>,
    axial_tilt_deg: Option<f32>,
    parent_entity: Option<Option<String>>,
}

impl PlanetBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn radius_km(mut self, radius_km: f32) -> Self {
        self.radius_km = Some(radius_km);
        self
    }

    pub fn mass_kg(mut self, mass_kg: f64) -> Self {
        self.mass_kg = Some(mass_kg);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn orbital_distance_au(mut self, orbital_distance_au: f32) -> Self {
        self.orbital_distance_au = Some(orbital_distance_au);
        self
    }

    pub fn orbital_period_days(mut self, orbital_period_days: f32) -> Self {
        self.orbital_period_days = Some(orbital_period_days);
        self
    }

    pub fn rotation_period_hours(mut self, rotation_period_hours: f32) -> Self {
        self.rotation_period_hours = Some(rotation_period_hours);
        self
    }

    pub fn axial_tilt_deg(mut self, axial_tilt_deg: f32) -> Self {
        self.axial_tilt_deg = Some(axial_tilt_deg);
        self
    }

    pub fn parent_entity(mut self, parent_entity: Option<String>) -> Self {
        self.parent_entity = Some(parent_entity);
        self
    }

    pub fn build(self) -> Planet {
        Planet {
            name: self.name.expect("name is required"),
            radius_km: self.radius_km.expect("radius_km is required"),
            mass_kg: self.mass_kg.expect("mass_kg is required"),
            color: self.color.expect("color is required"),
            orbital_distance_au: self.orbital_distance_au.expect("orbital_distance_au is required"),
            orbital_period_days: self.orbital_period_days.expect("orbital_period_days is required"),
            rotation_period_hours: self.rotation_period_hours.expect("rotation_period_hours is required"),
            axial_tilt_deg: self.axial_tilt_deg.expect("axial_tilt_deg is required"),
            parent_entity: self.parent_entity.expect("parent_entity is required"),
        }
    }
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
        axial_tilt_deg: f32,
        parent_entity: Option<String>,
    ) -> Self {
        Self {
            name,
            radius_km,
            mass_kg,
            color,
            orbital_distance_au,
            orbital_period_days,
            rotation_period_hours,
            axial_tilt_deg,
            parent_entity,
        }
    }


}
