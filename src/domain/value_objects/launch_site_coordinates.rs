use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use bevy::math::Vec3;
use bevy::prelude::Component;

/// Validation failure for a launch-site coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchSiteCoordinatesError {
    NonFiniteLatitude,
    LatitudeOutOfRange,
    NonFiniteLongitude,
    NonFiniteAltitude,
}

/// Coordinate system for launch sites using latitude, longitude, and altitude
/// This provides accurate positioning relative to planetary surfaces
#[derive(Component, Debug, Clone)]
pub struct LaunchSiteCoordinates {
    pub planet_id: CelestialBodyId,
    pub latitude_deg: f32,  // -90 to 90 degrees
    pub longitude_deg: f32, // Canonical [-180, 180) degrees
    pub altitude_m: f32,    // Height above reference ellipsoid in meters
}

impl LaunchSiteCoordinates {
    /// Create a validated launch-site coordinate.
    ///
    /// Panics when a coordinate is invalid. Use [`Self::try_new`] for input
    /// that may be invalid at runtime.
    pub fn new(
        planet_id: CelestialBodyId,
        latitude_deg: f32,
        longitude_deg: f32,
        altitude_m: f32,
    ) -> Self {
        Self::try_new(planet_id, latitude_deg, longitude_deg, altitude_m)
            .expect("launch-site coordinates must be finite and latitude must be within [-90, 90]")
    }

    /// Validate latitude, longitude, and altitude, then canonicalize longitude
    /// to the project's `[-180, 180)` degree convention.
    pub fn try_new(
        planet_id: CelestialBodyId,
        latitude_deg: f32,
        longitude_deg: f32,
        altitude_m: f32,
    ) -> Result<Self, LaunchSiteCoordinatesError> {
        if !latitude_deg.is_finite() {
            return Err(LaunchSiteCoordinatesError::NonFiniteLatitude);
        }
        if !(-90.0..=90.0).contains(&latitude_deg) {
            return Err(LaunchSiteCoordinatesError::LatitudeOutOfRange);
        }
        if !longitude_deg.is_finite() {
            return Err(LaunchSiteCoordinatesError::NonFiniteLongitude);
        }
        if !altitude_m.is_finite() {
            return Err(LaunchSiteCoordinatesError::NonFiniteAltitude);
        }

        Ok(Self {
            planet_id,
            latitude_deg,
            longitude_deg: (longitude_deg + 180.0).rem_euclid(360.0) - 180.0,
            altitude_m,
        })
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
            planet_id: CelestialBodyId::earth(),
            latitude_deg: 28.5721,   // Kennedy Space Center latitude
            longitude_deg: -80.6480, // Kennedy Space Center longitude
            altitude_m: 0.0,         // Sea level
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_normalizes_longitude_to_the_canonical_seam() {
        let site =
            LaunchSiteCoordinates::try_new(CelestialBodyId::earth(), 0.0, 540.0, -10.0).unwrap();

        assert_eq!(site.longitude_deg, -180.0);
        assert_eq!(site.altitude_m, -10.0);
    }

    #[test]
    fn constructor_rejects_non_finite_coordinates_and_invalid_latitude() {
        let planet = CelestialBodyId::earth();

        assert!(matches!(
            LaunchSiteCoordinates::try_new(planet.clone(), f32::NAN, 0.0, 0.0),
            Err(LaunchSiteCoordinatesError::NonFiniteLatitude)
        ));
        assert!(matches!(
            LaunchSiteCoordinates::try_new(planet.clone(), 91.0, 0.0, 0.0),
            Err(LaunchSiteCoordinatesError::LatitudeOutOfRange)
        ));
        assert!(matches!(
            LaunchSiteCoordinates::try_new(planet.clone(), 0.0, f32::INFINITY, 0.0),
            Err(LaunchSiteCoordinatesError::NonFiniteLongitude)
        ));
        assert!(matches!(
            LaunchSiteCoordinates::try_new(planet, 0.0, 0.0, f32::NEG_INFINITY),
            Err(LaunchSiteCoordinatesError::NonFiniteAltitude)
        ));
    }
}

/// Predefined launch site coordinates for common launch facilities
pub mod predefined_sites {
    use super::LaunchSiteCoordinates;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;

    pub fn kennedy_space_center() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            28.5721,  // Latitude
            -80.6480, // Longitude
            3.0,      // Altitude above sea level (meters)
        )
    }

    pub fn cape_canaveral() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            28.4889,  // Latitude
            -80.5778, // Longitude
            3.0,      // Altitude above sea level (meters)
        )
    }

    pub fn baikonur_cosmodrome() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            45.9650, // Latitude
            63.3050, // Longitude
            90.0,    // Altitude above sea level (meters)
        )
    }

    pub fn guiana_space_centre() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            5.2360,   // Latitude
            -52.7750, // Longitude
            10.0,     // Altitude above sea level (meters)
        )
    }
}
