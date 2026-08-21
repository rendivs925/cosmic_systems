use crate::domain::entities::planet::Planet;
use bevy::math::Vec3;
use bevy::prelude::Component;

/// Coordinate system for launch sites using latitude, longitude, and altitude
/// This provides accurate positioning relative to planetary surfaces
#[derive(Component, Debug, Clone)]
pub struct LaunchSiteCoordinates {
    pub planet_name: String,
    pub latitude_deg: f32,  // -90 to 90 degrees
    pub longitude_deg: f32, // -180 to 180 degrees
    pub altitude_m: f32,    // Height above reference ellipsoid in meters
}

impl LaunchSiteCoordinates {
    /// Create a new launch site coordinate
    pub fn new(
        planet_name: String,
        latitude_deg: f32,
        longitude_deg: f32,
        altitude_m: f32,
    ) -> Self {
        Self {
            planet_name,
            latitude_deg: latitude_deg.clamp(-90.0, 90.0),
            longitude_deg: longitude_deg.clamp(-180.0, 180.0),
            altitude_m,
        }
    }

    /// Convert lat/lon/alt coordinates to planet-centered Cartesian coordinates
    /// Accounts for ellipsoidal planet shape (not just spherical)
    pub fn to_planet_relative_position(&self, planet: &Planet) -> Vec3 {
        let planet_radius_km = planet.radius_km;
        let planet_radius_m = planet_radius_km * 1000.0;

        // Convert degrees to radians
        let lat_rad = self.latitude_deg.to_radians();
        let lon_rad = self.longitude_deg.to_radians();

        // For simplicity, treat as spherical. Advanced implementation would use ellipsoidal model
        // with different equatorial and polar radii

        // Convert to Cartesian coordinates (planet-centered)
        let x = (planet_radius_m + self.altitude_m) * lat_rad.cos() * lon_rad.cos();
        let y = (planet_radius_m + self.altitude_m) * lat_rad.sin();
        let z = (planet_radius_m + self.altitude_m) * lat_rad.cos() * lon_rad.sin();

        Vec3::new(x, y, z)
    }

    /// Calculate distance to another coordinate (on same planet)
    pub fn distance_to(&self, other: &LaunchSiteCoordinates, planet: &Planet) -> f32 {
        let pos1 = self.to_planet_relative_position(planet);
        let pos2 = other.to_planet_relative_position(planet);
        pos1.distance(pos2)
    }
}

impl Default for LaunchSiteCoordinates {
    fn default() -> Self {
        Self {
            planet_name: "Earth".to_string(),
            latitude_deg: 28.5721,   // Kennedy Space Center latitude
            longitude_deg: -80.6480, // Kennedy Space Center longitude
            altitude_m: 0.0,         // Sea level
        }
    }
}

/// Predefined launch site coordinates for common launch facilities
pub mod predefined_sites {
    use super::LaunchSiteCoordinates;

    pub fn kennedy_space_center() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            "Earth".to_string(),
            28.5721,  // Latitude
            -80.6480, // Longitude
            3.0,      // Altitude above sea level (meters)
        )
    }

    pub fn cape_canaveral() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            "Earth".to_string(),
            28.4889,  // Latitude
            -80.5778, // Longitude
            3.0,      // Altitude above sea level (meters)
        )
    }

    pub fn baikonur_cosmodrome() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            "Earth".to_string(),
            45.9650, // Latitude
            63.3050, // Longitude
            90.0,    // Altitude above sea level (meters)
        )
    }

    pub fn guiana_space_centre() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            "Earth".to_string(),
            5.2360,   // Latitude
            -52.7750, // Longitude
            10.0,     // Altitude above sea level (meters)
        )
    }
}
