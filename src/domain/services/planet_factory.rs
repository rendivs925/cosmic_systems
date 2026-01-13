use bevy::prelude::*;
use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::planet_configs::{PlanetConfig, PLANET_CONFIGS};

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

    /// Create planet from configuration
    fn create_from_config(config: &PlanetConfig) -> Planet {
        Planet::new(
            config.name.to_string(),
            config.radius_km,
            config.mass_kg,
            config.color,
            config.orbital_distance_au,
            config.orbital_period_days,
            config.rotation_period_hours,
            config.axial_tilt_deg,
            config.parent_entity.map(|s| s.to_string()),
        )
    }
}

// Convenience methods for backward compatibility
impl Planet {
    pub fn create_mercury() -> Self {
        PlanetFactory::create_by_name("Mercury").expect("Mercury config should exist")
    }

    pub fn create_venus() -> Self {
        PlanetFactory::create_by_name("Venus").expect("Venus config should exist")
    }

    pub fn create_earth() -> Self {
        PlanetFactory::create_by_name("Earth").expect("Earth config should exist")
    }

    pub fn create_mars() -> Self {
        PlanetFactory::create_by_name("Mars").expect("Mars config should exist")
    }

    pub fn create_jupiter() -> Self {
        PlanetFactory::create_by_name("Jupiter").expect("Jupiter config should exist")
    }

    pub fn create_saturn() -> Self {
        PlanetFactory::create_by_name("Saturn").expect("Saturn config should exist")
    }

    pub fn create_uranus() -> Self {
        PlanetFactory::create_by_name("Uranus").expect("Uranus config should exist")
    }

    pub fn create_neptune() -> Self {
        PlanetFactory::create_by_name("Neptune").expect("Neptune config should exist")
    }

    pub fn create_sun() -> Self {
        PlanetFactory::create_by_name("Sun").expect("Sun config should exist")
    }

    // Moon creation methods
    pub fn create_moon() -> Self {
        PlanetFactory::create_by_name("Moon").expect("Moon config should exist")
    }

    pub fn create_phobos() -> Self {
        PlanetFactory::create_by_name("Phobos").expect("Phobos config should exist")
    }

    pub fn create_deimos() -> Self {
        PlanetFactory::create_by_name("Deimos").expect("Deimos config should exist")
    }

    pub fn create_io() -> Self {
        PlanetFactory::create_by_name("Io").expect("Io config should exist")
    }

    pub fn create_europa() -> Self {
        PlanetFactory::create_by_name("Europa").expect("Europa config should exist")
    }

    pub fn create_ganymede() -> Self {
        PlanetFactory::create_by_name("Ganymede").expect("Ganymede config should exist")
    }

    pub fn create_callisto() -> Self {
        PlanetFactory::create_by_name("Callisto").expect("Callisto config should exist")
    }

    pub fn create_mimas() -> Self {
        PlanetFactory::create_by_name("Mimas").expect("Mimas config should exist")
    }

    pub fn create_enceladus() -> Self {
        PlanetFactory::create_by_name("Enceladus").expect("Enceladus config should exist")
    }

    pub fn create_tethys() -> Self {
        PlanetFactory::create_by_name("Tethys").expect("Tethys config should exist")
    }

    pub fn create_dione() -> Self {
        PlanetFactory::create_by_name("Dione").expect("Dione config should exist")
    }

    pub fn create_rhea() -> Self {
        PlanetFactory::create_by_name("Rhea").expect("Rhea config should exist")
    }

    pub fn create_titan() -> Self {
        PlanetFactory::create_by_name("Titan").expect("Titan config should exist")
    }

    pub fn create_hyperion() -> Self {
        PlanetFactory::create_by_name("Hyperion").expect("Hyperion config should exist")
    }

    pub fn create_iapetus() -> Self {
        PlanetFactory::create_by_name("Iapetus").expect("Iapetus config should exist")
    }

    pub fn create_miranda() -> Self {
        PlanetFactory::create_by_name("Miranda").expect("Miranda config should exist")
    }

    pub fn create_ariel() -> Self {
        PlanetFactory::create_by_name("Ariel").expect("Ariel config should exist")
    }

    pub fn create_umbriel() -> Self {
        PlanetFactory::create_by_name("Umbriel").expect("Umbriel config should exist")
    }

    pub fn create_titania() -> Self {
        PlanetFactory::create_by_name("Titania").expect("Titania config should exist")
    }

    pub fn create_oberon() -> Self {
        PlanetFactory::create_by_name("Oberon").expect("Oberon config should exist")
    }

    pub fn create_triton() -> Self {
        PlanetFactory::create_by_name("Triton").expect("Triton config should exist")
    }

    pub fn create_proteus() -> Self {
        PlanetFactory::create_by_name("Proteus").expect("Proteus config should exist")
    }

    pub fn create_nereid() -> Self {
        PlanetFactory::create_by_name("Nereid").expect("Nereid config should exist")
    }

    pub fn create_larissa() -> Self {
        PlanetFactory::create_by_name("Larissa").expect("Larissa config should exist")
    }

    pub fn create_charon() -> Self {
        PlanetFactory::create_by_name("Charon").expect("Charon config should exist")
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
        assert_eq!(moons.len(), 25); // All moons

        // Check that moons have parent entities
        for moon in &moons {
            assert!(moon.parent_entity.is_some(), "Moon {} should have a parent", moon.name);
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
        assert_eq!(names.len(), 34); // 9 planets + 25 moons

        assert!(names.contains(&"Earth"));
        assert!(names.contains(&"Moon"));
        assert!(names.contains(&"Mars"));
        assert!(names.contains(&"Phobos"));
        assert!(names.contains(&"Jupiter"));
        assert!(names.contains(&"Io"));
    }

    #[test]
    fn test_backward_compatibility_methods() {
        // Test that the old static methods still work
        let mercury = Planet::create_mercury();
        assert_eq!(mercury.name, "Mercury");

        let earth = Planet::create_earth();
        assert_eq!(earth.name, "Earth");

        let moon = Planet::create_moon();
        assert_eq!(moon.name, "Moon");
        assert_eq!(moon.parent_entity, Some("Earth".to_string()));

        let jupiter = Planet::create_jupiter();
        assert_eq!(jupiter.name, "Jupiter");

        let saturn = Planet::create_saturn();
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