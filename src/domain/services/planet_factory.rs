use crate::domain::entities::planet::{Planet, PlanetBuilder};
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::planet_configs::{PlanetConfig, PLANET_CONFIGS};
use bevy::prelude::*;

/// Factory service for creating celestial bodies
pub struct PlanetFactory;

impl PlanetFactory {
    /// Create a planet by name
    pub fn create_by_name(name: &str) -> Option<Planet> {
        PLANET_CONFIGS
            .iter()
            .find(|config| config.name == name)
            .map(Self::create_from_config)
    }

    /// Create a planet from an already validated domain identifier.
    pub fn create_by_id(id: &CelestialBodyId) -> Option<Planet> {
        Self::create_by_name(id.as_str())
    }

    /// Get all available planet names
    pub fn get_available_names() -> Vec<&'static str> {
        PLANET_CONFIGS.iter().map(|config| config.name).collect()
    }

    /// Get planets by parent entity (moons)
    pub fn get_moons_of(parent_name: &str) -> Vec<Planet> {
        PLANET_CONFIGS
            .iter()
            .filter(|config| config.parent_entity == Some(parent_name))
            .map(Self::create_from_config)
            .collect()
    }

    /// Get all planets (excluding moons)
    pub fn get_planets() -> Vec<Planet> {
        PLANET_CONFIGS
            .iter()
            .filter(|config| config.parent_entity.is_none())
            .map(Self::create_from_config)
            .collect()
    }

    /// Get all moons
    pub fn get_moons() -> Vec<Planet> {
        PLANET_CONFIGS
            .iter()
            .filter(|config| config.parent_entity.is_some())
            .map(Self::create_from_config)
            .collect()
    }

    /// Create planet from configuration — every field, including the ocean
    /// mask, comes from the config table; nothing is inferred here.
    fn create_from_config(config: &PlanetConfig) -> Planet {
        PlanetBuilder::new()
            .name(config.name.to_string())
            .radius_km(config.radius_km)
            .mass_kg(config.mass_kg)
            .color(config.color)
            .body_class(config.body_class)
            .orbital_distance_au(config.orbital_distance_au)
            .orbital_period_days(config.orbital_period_days)
            .rotation_period_hours(config.rotation_period_hours)
            .axial_tilt_deg(config.axial_tilt_deg)
            .parent_entity(config.parent_entity.map(|s| s.to_string()))
            .has_ocean(config.has_ocean)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planet_factory_creates_planets_by_name() {
        // Test planets
        let mercury = PlanetFactory::create_by_name("Mercury").unwrap();
        assert_eq!(mercury.name, "Mercury");
        assert_eq!(mercury.radius_km, 2439.7);
        assert!(mercury.parent_entity.is_none()); // Mercury orbits Sun

        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        assert_eq!(earth.name, "Earth");
        assert_eq!(earth.radius_km, 6371.0);
        assert!(earth.parent_entity.is_none()); // Earth orbits Sun

        // Test moons
        let moon = PlanetFactory::create_by_name("Moon").unwrap();
        assert_eq!(moon.name, "Moon");
        assert_eq!(moon.parent_entity, Some("Earth".to_string()));

        let phobos = PlanetFactory::create_by_name("Phobos").unwrap();
        assert_eq!(phobos.name, "Phobos");
        assert_eq!(phobos.parent_entity, Some("Mars".to_string()));
    }

    #[test]
    fn test_planet_factory_creates_planets_by_id() {
        let earth_id = CelestialBodyId::earth();
        let earth = PlanetFactory::create_by_id(&earth_id).unwrap();
        assert_eq!(earth.name, earth_id.as_str());
    }

    #[test]
    fn test_planet_factory_get_planets() {
        let planets = PlanetFactory::get_planets();
        assert_eq!(planets.len(), 9); // Sun + 8 planets

        // Check that we have the Sun and all planets
        let names: Vec<&str> = planets.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Sun"));
        assert!(names.contains(&"Mercury"));
        assert!(names.contains(&"Venus"));
        assert!(names.contains(&"Earth"));
        assert!(names.contains(&"Mars"));
        assert!(names.contains(&"Jupiter"));
        assert!(names.contains(&"Saturn"));
        assert!(names.contains(&"Uranus"));
        assert!(names.contains(&"Neptune"));
    }

    #[test]
    fn test_planet_factory_get_moons() {
        let moons = PlanetFactory::get_moons();
        assert_eq!(moons.len(), 24); // All moons with simulated parent bodies

        // Check that moons have parent entities
        for moon in &moons {
            assert!(
                moon.parent_entity.is_some(),
                "Moon {} should have a parent",
                moon.name
            );
        }
    }

    #[test]
    fn test_planet_factory_get_moons_of() {
        let earth_moons = PlanetFactory::get_moons_of("Earth");
        assert_eq!(earth_moons.len(), 1);
        assert_eq!(earth_moons[0].name, "Moon");

        let mars_moons = PlanetFactory::get_moons_of("Mars");
        assert_eq!(mars_moons.len(), 2);
        let mars_moon_names: Vec<&str> = mars_moons.iter().map(|m| m.name.as_str()).collect();
        assert!(mars_moon_names.contains(&"Phobos"));
        assert!(mars_moon_names.contains(&"Deimos"));

        let non_existent = PlanetFactory::get_moons_of("NonExistentPlanet");
        assert_eq!(non_existent.len(), 0);
    }

    #[test]
    fn test_planet_factory_get_available_names() {
        let names = PlanetFactory::get_available_names();
        assert_eq!(names.len(), 33); // 9 planets + 24 moons

        assert!(names.contains(&"Earth"));
        assert!(names.contains(&"Moon"));
        assert!(names.contains(&"Mars"));
        assert!(names.contains(&"Phobos"));
        assert!(names.contains(&"Jupiter"));
        assert!(names.contains(&"Io"));
    }

    #[test]
    fn test_factory_methods_work() {
        // Test that the factory methods work correctly
        let mercury = PlanetFactory::create_by_name("Mercury").unwrap();
        assert_eq!(mercury.name, "Mercury");

        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        assert_eq!(earth.name, "Earth");

        let moon = PlanetFactory::create_by_name("Moon").unwrap();
        assert_eq!(moon.name, "Moon");
        assert_eq!(moon.parent_entity, Some("Earth".to_string()));

        let jupiter = PlanetFactory::create_by_name("Jupiter").unwrap();
        assert_eq!(jupiter.name, "Jupiter");

        let saturn = PlanetFactory::create_by_name("Saturn").unwrap();
        assert_eq!(saturn.name, "Saturn");
    }

    #[test]
    fn test_invalid_planet_name() {
        let result = PlanetFactory::create_by_name("NonExistentPlanet");
        assert!(result.is_none());
    }

    #[test]
    fn test_planet_properties_accuracy() {
        // Test that planet properties match astronomical data
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        assert_eq!(earth.radius_km, 6371.0);
        assert_eq!(earth.mass_kg, 5.97237e24);
        assert_eq!(earth.orbital_distance_au, 1.000);
        assert_eq!(earth.orbital_period_days, 365.256);
        assert_eq!(earth.rotation_period_hours, 23.934);
        assert_eq!(earth.axial_tilt_deg, 23.439);

        let moon = PlanetFactory::create_by_name("Moon").unwrap();
        assert_eq!(moon.radius_km, 1737.4);
        assert_eq!(moon.mass_kg, 7.342e22);
        assert_eq!(moon.orbital_distance_au, 0.002569);
        assert_eq!(moon.parent_entity, Some("Earth".to_string()));
    }
}
