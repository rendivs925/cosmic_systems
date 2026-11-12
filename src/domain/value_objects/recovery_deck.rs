//! Sea-surface recovery-deck coordinates.
//!
//! The deck is a point on the rendered/terrain body-fixed sphere, so its
//! latitude is the **terrain-radial** latitude used by the terrain source and
//! by the capture/diagnostic tools — not the WGS-84 geodetic latitude of a
//! launch site. The two differ by about 0.15 degrees at mid-latitudes (roughly
//! 17 km), which is larger than a drone-ship deck and would place the platform
//! off the measured ground track.

use crate::domain::math::DVec3;

/// Validation failure for a recovery-deck coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDeckError {
    NonFiniteLatitude,
    LatitudeOutOfRange,
    NonFiniteLongitude,
}

/// A body-fixed, terrain-radial sea-surface point for a landing deck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecoveryDeckCoordinates {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
}

impl RecoveryDeckCoordinates {
    /// Validate the coordinate. Latitude is terrain-radial in degrees and
    /// longitude follows the terrain convention `[-180, 180)`.
    pub fn try_new(latitude_deg: f64, longitude_deg: f64) -> Result<Self, RecoveryDeckError> {
        if !latitude_deg.is_finite() {
            return Err(RecoveryDeckError::NonFiniteLatitude);
        }
        if !(-90.0..=90.0).contains(&latitude_deg) {
            return Err(RecoveryDeckError::LatitudeOutOfRange);
        }
        if !longitude_deg.is_finite() {
            return Err(RecoveryDeckError::NonFiniteLongitude);
        }
        Ok(Self {
            latitude_deg,
            longitude_deg,
        })
    }

    /// Body-fixed position on the terrain sphere of `reference_radius_m`.
    /// The axes match the terrain source: +X at longitude 0, +Y north, +Z east.
    pub fn position_body_fixed(self, reference_radius_m: f64) -> DVec3 {
        let latitude_rad = self.latitude_deg.to_radians();
        let longitude_rad = self.longitude_deg.to_radians();
        DVec3::new(
            latitude_rad.cos() * longitude_rad.cos(),
            latitude_rad.sin(),
            latitude_rad.cos() * longitude_rad.sin(),
        ) * reference_radius_m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_position_is_the_terrain_radial_direction() {
        let deck = RecoveryDeckCoordinates::try_new(25.57427, -89.31169).expect("valid deck");
        let position = deck.position_body_fixed(6_371_000.0);
        let recovered = crate::domain::services::reference_frames::body_fixed_to_terrain_lat_lon(
            position.normalize(),
        );
        assert!((recovered.0 - 25.57427).abs() < 1e-9);
        assert!((recovered.1 + 89.31169).abs() < 1e-9);
        assert!((position.length() - 6_371_000.0).abs() < 1e-6);
    }

    #[test]
    fn deck_rejects_out_of_range_coordinates() {
        assert_eq!(
            RecoveryDeckCoordinates::try_new(91.0, 0.0),
            Err(RecoveryDeckError::LatitudeOutOfRange)
        );
        assert_eq!(
            RecoveryDeckCoordinates::try_new(f64::NAN, 0.0),
            Err(RecoveryDeckError::NonFiniteLatitude)
        );
        assert_eq!(
            RecoveryDeckCoordinates::try_new(0.0, f64::INFINITY),
            Err(RecoveryDeckError::NonFiniteLongitude)
        );
    }
}
