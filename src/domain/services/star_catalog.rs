//! Versioned offline bright-star catalogs for presentation-only skies.
//!
//! A `.csstar` package stores ICRS/J2000 right ascension and declination in
//! radians, V magnitude, and B-V colour index for a fixed star list. The
//! package is frame-neutral: the presentation layer applies the authoritative
//! [`icrf_j2000_to_solar_inertial`] conversion so stars share the solar
//! system's axes.
//!
//! Star positions are presentation data only. No orbital, attitude, terrain,
//! collision, or physics system reads this catalog.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::fs;
use std::io;
use std::path::Path;

use crate::domain::math::DVec3;
use crate::domain::services::reference_frames::icrf_j2000_to_solar_inertial;

const MAGIC: [u8; 8] = *b"CSSTAR\0\0";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 16;
const RECORD_BYTES: usize = 16;
/// A catalog with more records than this is almost certainly mislabelled.
const MAX_RECORDS: usize = 10_000_000;

/// Default native asset holding the baked Yale Bright Star Catalog.
pub const DEFAULT_STAR_CATALOG_PATH: &str = "assets/data/stars/yale_bsc5_v1.csstar";

/// The reviewed BSC5 package embedded at build time. The asset also remains in
/// the repository for provenance and inspection; embedding keeps startup free
/// of a presentation-critical file dependency on every target.
pub const EMBEDDED_YALE_BSC5_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/data/stars/yale_bsc5_v1.csstar"
));

/// Failures when validating or decoding a star catalog.
#[derive(Debug)]
pub enum StarCatalogError {
    Io(io::Error),
    InvalidFormat(String),
}

impl From<io::Error> for StarCatalogError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// One catalog star in the source frame and photometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarRecord {
    /// ICRS right ascension, radians in `[0, 2*pi)`.
    pub ra_rad: f32,
    /// ICRS declination, radians in `[-pi/2, pi/2]`.
    pub dec_rad: f32,
    /// Johnson V apparent magnitude.
    pub v_mag: f32,
    /// Johnson B-V colour index; lower is bluer.
    pub bv_color: f32,
}

impl StarRecord {
    /// Unit direction in the ICRS/J2000 equatorial frame.
    pub fn icrs_direction(&self) -> DVec3 {
        let (sin_ra, cos_ra) = f64::from(self.ra_rad).sin_cos();
        let (sin_dec, cos_dec) = f64::from(self.dec_rad).sin_cos();
        DVec3::new(cos_dec * cos_ra, cos_dec * sin_ra, sin_dec)
    }

    /// Unit direction in the project's solar-inertial axes, matching the axis
    /// mapping used for every ephemeris position in the scene.
    pub fn solar_inertial_direction(&self) -> DVec3 {
        icrf_j2000_to_solar_inertial(self.icrs_direction())
    }
}

/// An immutable bright-star catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct StarCatalog {
    stars: Vec<StarRecord>,
}

impl StarCatalog {
    /// Decode and validate a `.csstar` package.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StarCatalogError> {
        if bytes.len() < HEADER_BYTES || bytes[..8] != MAGIC {
            return Err(StarCatalogError::InvalidFormat(
                "invalid star catalog header".into(),
            ));
        }
        let u32_at =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if u32_at(8) != VERSION {
            return Err(StarCatalogError::InvalidFormat(
                "unsupported star catalog version".into(),
            ));
        }
        let count = u32_at(12) as usize;
        if count == 0 || count > MAX_RECORDS {
            return Err(StarCatalogError::InvalidFormat(
                "star catalog count is out of range".into(),
            ));
        }
        let expected_bytes = count
            .checked_mul(RECORD_BYTES)
            .and_then(|payload| payload.checked_add(HEADER_BYTES))
            .ok_or_else(|| {
                StarCatalogError::InvalidFormat("star catalog length overflows".into())
            })?;
        if bytes.len() != expected_bytes {
            return Err(StarCatalogError::InvalidFormat(format!(
                "star catalog payload must be {expected_bytes} bytes for {count} records, found {}",
                bytes.len()
            )));
        }

        let mut stars = Vec::with_capacity(count);
        for record in bytes[HEADER_BYTES..].chunks_exact(RECORD_BYTES) {
            let f32_at =
                |offset: usize| f32::from_le_bytes(record[offset..offset + 4].try_into().unwrap());
            let star = StarRecord {
                ra_rad: f32_at(0),
                dec_rad: f32_at(4),
                v_mag: f32_at(8),
                bv_color: f32_at(12),
            };
            if !star.ra_rad.is_finite()
                || !(0.0..TAU).contains(&star.ra_rad)
                || !star.dec_rad.is_finite()
                || !(-FRAC_PI_2..=FRAC_PI_2).contains(&star.dec_rad)
                || !(-5.0..=15.0).contains(&star.v_mag)
                || !(-2.0..=5.0).contains(&star.bv_color)
            {
                return Err(StarCatalogError::InvalidFormat(
                    "star catalog record is out of range".into(),
                ));
            }
            stars.push(star);
        }
        Ok(Self { stars })
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, StarCatalogError> {
        Self::from_bytes(&fs::read(path)?)
    }

    /// The reviewed Yale BSC5 package compiled into the binary.
    pub fn embedded_yale_bsc5() -> Result<Self, StarCatalogError> {
        Self::from_bytes(EMBEDDED_YALE_BSC5_BYTES)
    }

    pub fn stars(&self) -> &[StarRecord] {
        &self.stars
    }

    pub fn len(&self) -> usize {
        self.stars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stars.is_empty()
    }
}

/// Orthonormal basis of the IAU galactic frame expressed in the project's
/// solar-inertial axes.
///
/// The frame follows the J2000 adoption of the IAU 1958 galactic pole and
/// centre, as used for Hipparcos and Gaia: the north pole is at ICRS
/// RA 192.85948 deg, Dec +27.12825 deg and the centre at RA 266.40510 deg,
/// Dec -28.936175 deg. Directions are built from the ICRS basis and then
/// mapped once, so the Milky Way band agrees with the mapped star directions
/// even though the project's solar axes are a reflected ordering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GalacticFrame {
    /// Galactic longitude zero, latitude zero (galactic centre).
    pub center: DVec3,
    /// Galactic longitude 90 degrees on the galactic equator.
    pub east: DVec3,
    /// Galactic north pole.
    pub north_pole: DVec3,
}

const GALACTIC_POLE_RA_DEG: f64 = 192.859_48;
const GALACTIC_POLE_DEC_DEG: f64 = 27.128_25;
const GALACTIC_CENTER_RA_DEG: f64 = 266.405_10;
const GALACTIC_CENTER_DEC_DEG: f64 = -28.936_175;

fn icrs_direction(ra_deg: f64, dec_deg: f64) -> DVec3 {
    let (sin_ra, cos_ra) = ra_deg.to_radians().sin_cos();
    let (sin_dec, cos_dec) = dec_deg.to_radians().sin_cos();
    DVec3::new(cos_dec * cos_ra, cos_dec * sin_ra, sin_dec)
}

/// Build the galactic basis in the project's solar-inertial axes.
pub fn galactic_frame_solar_inertial() -> GalacticFrame {
    let north_pole_icrs = icrs_direction(GALACTIC_POLE_RA_DEG, GALACTIC_POLE_DEC_DEG);
    let center_icrs = icrs_direction(GALACTIC_CENTER_RA_DEG, GALACTIC_CENTER_DEC_DEG);
    let center_icrs =
        (center_icrs - north_pole_icrs * center_icrs.dot(north_pole_icrs)).normalize();
    let east_icrs = north_pole_icrs.cross(center_icrs).normalize();
    GalacticFrame {
        center: icrf_j2000_to_solar_inertial(center_icrs),
        east: icrf_j2000_to_solar_inertial(east_icrs),
        north_pole: icrf_j2000_to_solar_inertial(north_pole_icrs),
    }
}

impl GalacticFrame {
    /// Unit direction for a galactic longitude and latitude in radians.
    pub fn direction(&self, longitude_rad: f64, latitude_rad: f64) -> DVec3 {
        let (sin_longitude, cos_longitude) = longitude_rad.sin_cos();
        let (sin_latitude, cos_latitude) = latitude_rad.sin_cos();
        self.center * (cos_latitude * cos_longitude)
            + self.east * (cos_latitude * sin_longitude)
            + self.north_pole * sin_latitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2 as FRAC_PI_2_F64;

    fn package(records: &[(f32, f32, f32, f32)]) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(records.len() as u32).to_le_bytes());
        for (ra, dec, v_mag, bv) in records {
            for value in [ra, dec, v_mag, bv] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn catalog_round_trips_records() {
        let bytes = package(&[(1.0, 0.2, 4.5, 0.65), (2.0, -0.4, -1.46, 0.0)]);
        let catalog = StarCatalog::from_bytes(&bytes).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog.stars()[1].v_mag, -1.46);
    }

    #[test]
    fn catalog_rejects_truncated_versions_and_out_of_range_records() {
        let valid = package(&[(1.0, 0.2, 4.5, 0.65)]);
        assert!(StarCatalog::from_bytes(&valid[..valid.len() - 1]).is_err());
        let mut wrong_version = valid.clone();
        wrong_version[8..12].copy_from_slice(&7_u32.to_le_bytes());
        assert!(StarCatalog::from_bytes(&wrong_version).is_err());
        let mut bad_declination = valid;
        bad_declination[20..24].copy_from_slice(&2.0_f32.to_le_bytes());
        assert!(StarCatalog::from_bytes(&bad_declination).is_err());
    }

    #[test]
    fn embedded_bright_star_catalog_is_intact_and_contains_sirius() {
        let catalog = StarCatalog::embedded_yale_bsc5().unwrap();
        assert_eq!(catalog.len(), 9_110);
        let brightest = catalog
            .stars()
            .iter()
            .min_by(|left, right| left.v_mag.total_cmp(&right.v_mag))
            .unwrap();
        assert!((brightest.v_mag - -1.46).abs() < 1.0e-6);
        // Sirius: ICRS RA 101.287 deg, Dec -16.716 deg.
        let sirius = catalog
            .stars()
            .iter()
            .find(|star| (star.ra_rad.to_degrees() - 101.287).abs() < 0.01)
            .unwrap();
        assert!((sirius.dec_rad.to_degrees() - -16.716).abs() < 0.01);
        for star in catalog.stars() {
            assert!((star.icrs_direction().length() - 1.0).abs() < 1.0e-12);
        }
    }

    #[test]
    fn galactic_frame_matches_its_defining_directions() {
        let frame = galactic_frame_solar_inertial();
        assert!((frame.center.length() - 1.0).abs() < 1.0e-12);
        assert!((frame.east.length() - 1.0).abs() < 1.0e-12);
        assert!((frame.north_pole.length() - 1.0).abs() < 1.0e-12);
        assert!(frame.center.dot(frame.east).abs() < 1.0e-12);
        assert!(frame.center.dot(frame.north_pole).abs() < 1.0e-12);
        assert!((frame.direction(0.0, 0.0) - frame.center).length() < 1.0e-12);
        assert!((frame.direction(FRAC_PI_2_F64, 0.0) - frame.east).length() < 1.0e-12);
        assert!((frame.direction(0.0, FRAC_PI_2_F64) - frame.north_pole).length() < 1.0e-12);
        // The centre must equal the published ICRS direction after mapping.
        let published = icrs_direction(GALACTIC_CENTER_RA_DEG, GALACTIC_CENTER_DEC_DEG);
        let mapped = icrf_j2000_to_solar_inertial(published);
        assert!((frame.center - mapped).length() < 1.0e-4);
    }
}
