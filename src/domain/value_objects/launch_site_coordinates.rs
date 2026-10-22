use crate::domain::value_objects::celestial_body_id::CelestialBodyId;

/// Validation failure for a launch-site coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchSiteCoordinatesError {
    NonFiniteLatitude,
    LatitudeOutOfRange,
    NonFiniteLongitude,
    NonFiniteAltitude,
}

/// Coordinate system for launch sites using latitude, longitude, and altitude.
/// The authoritative Cartesian conversion lives in `reference_frames`.
#[derive(Debug, Clone)]
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
}

impl Default for LaunchSiteCoordinates {
    fn default() -> Self {
        Self {
            planet_id: CelestialBodyId::earth(),
            latitude_deg: 28.5721, // Legacy KSC reference used by physics baselines
            longitude_deg: -80.6480, // Legacy KSC reference longitude
            altitude_m: 3.0,       // Reference altitude above sea level
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

    #[test]
    fn predefined_site_ids_parse_and_round_trip() {
        for site in PredefinedLaunchSite::ALL {
            assert_eq!(PredefinedLaunchSite::parse(site.id()), Some(site));
            assert_eq!(site.coordinates().planet_id, CelestialBodyId::earth());
        }
        assert_eq!(
            PredefinedLaunchSite::parse("ksc"),
            Some(PredefinedLaunchSite::KennedyLc39a)
        );
        assert_eq!(
            PredefinedLaunchSite::parse("ksc-reference"),
            Some(PredefinedLaunchSite::KennedyReference)
        );
        assert_eq!(PredefinedLaunchSite::parse("unknown"), None);
    }

    #[test]
    fn papua_coastal_lowland_presentation_site_has_stable_coordinates() {
        let site = predefined_sites::papua_indonesia_coastal_lowland();

        assert_eq!(site.planet_id, CelestialBodyId::earth());
        assert_eq!(site.latitude_deg, -8.0);
        assert_eq!(site.longitude_deg, 139.5);
        assert_eq!(site.altitude_m, 0.0);
    }
}

/// Launch sites with curated coordinates and terrain coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PredefinedLaunchSite {
    #[default]
    Papua,
    KennedyReference,
    KennedyLc39a,
    Starbase,
    Vandenberg,
    Wallops,
    Kourou,
    Baikonur,
    Mahia,
}

impl PredefinedLaunchSite {
    pub const ALL: [Self; 9] = [
        Self::Papua,
        Self::KennedyReference,
        Self::KennedyLc39a,
        Self::Starbase,
        Self::Vandenberg,
        Self::Wallops,
        Self::Kourou,
        Self::Baikonur,
        Self::Mahia,
    ];

    /// Stable identity used by run artifacts and telemetry.
    pub fn id(self) -> &'static str {
        match self {
            Self::Papua => "papua-indonesia-coastal-lowland",
            Self::KennedyReference => "ksc-reference",
            Self::KennedyLc39a => "ksc-florida",
            Self::Starbase => "starbase-boca-chica",
            Self::Vandenberg => "vandenberg-california",
            Self::Wallops => "wallops-virginia",
            Self::Kourou => "kourou-french-guiana",
            Self::Baikonur => "baikonur-kazakhstan",
            Self::Mahia => "mahia-new-zealand",
        }
    }

    /// Human-readable name for logs and errors.
    pub fn label(self) -> &'static str {
        match self {
            Self::Papua => "Papua coastal lowland",
            Self::KennedyReference => "KSC legacy reference",
            Self::KennedyLc39a => "Kennedy Space Center LC-39A",
            Self::Starbase => "Starbase Boca Chica",
            Self::Vandenberg => "Vandenberg",
            Self::Wallops => "Wallops",
            Self::Kourou => "Kourou",
            Self::Baikonur => "Baikonur",
            Self::Mahia => "Mahia",
        }
    }

    /// Parse a stable ID or short CLI alias case-insensitively.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "papua" | "papua-indonesia-coastal-lowland" => Some(Self::Papua),
            "ksc" | "ksc-florida" | "lc39a" | "ksc-lc39a" => Some(Self::KennedyLc39a),
            "ksc-reference" => Some(Self::KennedyReference),
            "starbase" | "starbase-boca-chica" => Some(Self::Starbase),
            "vandenberg" | "vandenberg-california" => Some(Self::Vandenberg),
            "wallops" | "wallops-virginia" => Some(Self::Wallops),
            "kourou" | "kourou-french-guiana" => Some(Self::Kourou),
            "baikonur" | "baikonur-kazakhstan" => Some(Self::Baikonur),
            "mahia" | "mahia-new-zealand" => Some(Self::Mahia),
            _ => None,
        }
    }

    pub fn coordinates(self) -> LaunchSiteCoordinates {
        match self {
            Self::Papua => predefined_sites::papua_indonesia_coastal_lowland(),
            Self::KennedyReference => predefined_sites::kennedy_space_center(),
            Self::KennedyLc39a => predefined_sites::kennedy_lc39a(),
            Self::Starbase => predefined_sites::starbase_boca_chica(),
            Self::Vandenberg => predefined_sites::vandenberg_california(),
            Self::Wallops => predefined_sites::wallops_virginia(),
            Self::Kourou => predefined_sites::kourou_french_guiana(),
            Self::Baikonur => predefined_sites::baikonur_kazakhstan(),
            Self::Mahia => predefined_sites::mahia_new_zealand(),
        }
    }
}

/// Predefined launch-site coordinate presets.
pub mod predefined_sites {
    use super::LaunchSiteCoordinates;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;

    /// Legacy KSC coastal reference used by deterministic physics baselines.
    pub fn kennedy_space_center() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), 28.5721, -80.6480, 3.0)
    }

    /// Kennedy Space Center LC-39A pad reference.
    pub fn kennedy_lc39a() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), 28.6084, -80.6043, 14.0)
    }

    pub fn starbase_boca_chica() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), 25.9971, -97.1553, 3.0)
    }

    pub fn vandenberg_california() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), 34.742, -120.572, 100.0)
    }

    pub fn wallops_virginia() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), 37.9402, -75.4665, 3.0)
    }

    pub fn kourou_french_guiana() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), 5.159, -52.649, 3.0)
    }

    pub fn baikonur_kazakhstan() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), 45.965, 63.305, 80.0)
    }

    pub fn mahia_new_zealand() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(CelestialBodyId::earth(), -39.26, 177.864, 58.0)
    }

    /// Remote South Papua coastal-lowland presentation site, not a real spaceport.
    pub fn papua_indonesia_coastal_lowland() -> LaunchSiteCoordinates {
        LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            -8.0,  // Latitude
            139.5, // Longitude
            0.0,   // Terrain elevation is sampled at startup.
        )
    }
}
