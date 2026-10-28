//! Spherical-harmonic gravity for measured planetary gravity fields.
//!
//! This module extends the single gravity authority in
//! [`crate::domain::services::gravity`] with a full-field synthesizer. It does
//! not replace the point-mass or degree-two functions, and no default force
//! model selects it; callers must opt in through an explicit
//! [`crate::domain::services::gravity::ForceModelTerm`].
//!
//! A `.csgravc` package stores fully normalized (4π) coefficients in the IAU
//! body-fixed axes used by the shared orientation snapshot: `+Z` is the spin
//! axis and `+X` is the prime meridian. The potential is
//!
//! ```text
//! U = μ/r · Σ (a/r)^n · P̄_nm(sin φ) · [C̄_nm cos(mλ) + S̄_nm sin(mλ)]
//! ```
//!
//! and the returned acceleration is the gradient of its `n >= 2` terms.
//! Point mass remains the caller's existing `gravitational_acceleration_from_mu`
//! contribution. Coefficients are geodetic fully normalized, so degree 360 uses
//! bounded values instead of the overflowing unnormalized factorial products.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::math::DVec3;

const MAGIC: [u8; 8] = *b"CSGRAV\0\0";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 64;
const RECORD_BYTES: usize = 16;
/// The maximum degree published by EGM2008. Packages above this are rejected.
const MAX_DEGREE: u32 = 2_190;

/// Failures when validating or decoding a gravity-harmonics package.
#[derive(Debug)]
pub enum GravityHarmonicsError {
    Io(io::Error),
    InvalidFormat(String),
}

impl From<io::Error> for GravityHarmonicsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Provenance required for an accepted coefficient package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GravityHarmonicsMetadata {
    pub model_id: String,
    pub body: String,
    pub source: String,
    pub source_url: String,
    pub source_sha256: String,
    /// Must be `fully-normalized-4pi`; other conventions are converted by the
    /// ingest tool before baking.
    pub normalization: String,
    pub tide_system: String,
    pub license: String,
    pub attribution: String,
    pub conversion_version: u32,
}

impl GravityHarmonicsMetadata {
    fn is_valid(&self) -> bool {
        !self.model_id.trim().is_empty()
            && !self.body.trim().is_empty()
            && !self.source.trim().is_empty()
            && self.source_url.starts_with("https://")
            && self.source_sha256.len() == 64
            && self
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            && self.normalization == "fully-normalized-4pi"
            && !self.tide_system.trim().is_empty()
            && !self.license.trim().is_empty()
            && !self.attribution.trim().is_empty()
            && self.conversion_version > 0
    }
}

/// Immutable fully normalized spherical-harmonic gravity field.
#[derive(Debug, Clone, PartialEq)]
pub struct SphericalHarmonicGravityModel {
    degree: u32,
    reference_radius_m: f64,
    metadata: GravityHarmonicsMetadata,
    /// Interleaved `C̄_nm, S̄_nm` pairs for `n = 2..=degree`, `m = 0..=n`.
    coefficients: Vec<f64>,
}

/// Number of `(n, m)` coefficient pairs from degree 2 through `degree`.
pub const fn coefficient_pair_count(degree: u32) -> usize {
    (degree as usize) * (degree as usize + 3) / 2 - 2
}

/// Offset of the first pair for degree `n` in the triangular payload.
const fn degree_offset(n: u32) -> usize {
    (n as usize) * (n as usize + 1) / 2 - 3
}

impl SphericalHarmonicGravityModel {
    pub fn from_parts(
        degree: u32,
        reference_radius_m: f64,
        metadata: GravityHarmonicsMetadata,
        coefficients: Vec<f64>,
    ) -> Result<Self, GravityHarmonicsError> {
        if !(2..=MAX_DEGREE).contains(&degree) {
            return Err(GravityHarmonicsError::InvalidFormat(
                "gravity harmonics degree is out of range".into(),
            ));
        }
        let expected = coefficient_pair_count(degree)
            .checked_mul(2)
            .ok_or_else(|| {
                GravityHarmonicsError::InvalidFormat("gravity coefficient count overflows".into())
            })?;
        if !reference_radius_m.is_finite()
            || reference_radius_m <= 0.0
            || !metadata.is_valid()
            || coefficients.len() != expected
            || coefficients
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 10.0)
        {
            return Err(GravityHarmonicsError::InvalidFormat(
                "invalid gravity harmonics model".into(),
            ));
        }
        Ok(Self {
            degree,
            reference_radius_m,
            metadata,
            coefficients,
        })
    }

    /// Decode and validate a `.csgravc` package.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GravityHarmonicsError> {
        if bytes.len() < HEADER_BYTES || bytes[..8] != MAGIC {
            return Err(GravityHarmonicsError::InvalidFormat(
                "invalid gravity harmonics header".into(),
            ));
        }
        let u32_at =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if u32_at(8) != VERSION {
            return Err(GravityHarmonicsError::InvalidFormat(
                "unsupported gravity harmonics version".into(),
            ));
        }
        let metadata_bytes = u32_at(12) as usize;
        let degree = u32_at(16);
        if !(2..=MAX_DEGREE).contains(&degree) {
            return Err(GravityHarmonicsError::InvalidFormat(
                "gravity harmonics degree is out of range".into(),
            ));
        }
        let reference_radius_m = f64::from_le_bytes(bytes[24..32].try_into().unwrap());
        let payload_bytes = coefficient_pair_count(degree)
            .checked_mul(RECORD_BYTES)
            .ok_or_else(|| {
                GravityHarmonicsError::InvalidFormat("gravity payload length overflows".into())
            })?;
        let metadata_end = HEADER_BYTES.checked_add(metadata_bytes).ok_or_else(|| {
            GravityHarmonicsError::InvalidFormat("gravity metadata length overflows".into())
        })?;
        let expected_bytes = metadata_end.checked_add(payload_bytes).ok_or_else(|| {
            GravityHarmonicsError::InvalidFormat("gravity package length overflows".into())
        })?;
        if bytes.len() != expected_bytes {
            return Err(GravityHarmonicsError::InvalidFormat(
                "invalid gravity harmonics payload length".into(),
            ));
        }
        let payload = &bytes[metadata_end..];
        if Sha256::digest(payload).as_slice() != &bytes[32..64] {
            return Err(GravityHarmonicsError::InvalidFormat(
                "gravity harmonics checksum mismatch".into(),
            ));
        }
        let metadata = ron::from_str(
            std::str::from_utf8(&bytes[HEADER_BYTES..metadata_end]).map_err(|_| {
                GravityHarmonicsError::InvalidFormat("gravity metadata is not UTF-8".into())
            })?,
        )
        .map_err(|error| {
            GravityHarmonicsError::InvalidFormat(format!("invalid gravity metadata: {error}"))
        })?;
        let (pairs, remainder) = payload.as_chunks::<RECORD_BYTES>();
        debug_assert!(remainder.is_empty());
        let coefficients = pairs
            .iter()
            .flat_map(|pair| {
                [
                    f64::from_le_bytes(pair[0..8].try_into().unwrap()),
                    f64::from_le_bytes(pair[8..16].try_into().unwrap()),
                ]
            })
            .collect();
        Self::from_parts(degree, reference_radius_m, metadata, coefficients)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, GravityHarmonicsError> {
        Self::from_bytes(&fs::read(path)?)
    }

    /// Serialize to the versioned package format.
    pub fn write_bytes(&self) -> Vec<u8> {
        let metadata = ron::to_string(&self.metadata).expect("gravity metadata is serializable");
        let mut payload = Vec::with_capacity(self.coefficients.len() * 8);
        for value in &self.coefficients {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let mut bytes = Vec::with_capacity(HEADER_BYTES + metadata.len() + payload.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.degree.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&self.reference_radius_m.to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(&payload));
        bytes.extend_from_slice(metadata.as_bytes());
        bytes.extend_from_slice(&payload);
        bytes
    }

    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<(), GravityHarmonicsError> {
        fs::write(path, self.write_bytes())?;
        Ok(())
    }

    pub fn degree(&self) -> u32 {
        self.degree
    }

    pub fn reference_radius_m(&self) -> f64 {
        self.reference_radius_m
    }

    pub fn metadata(&self) -> &GravityHarmonicsMetadata {
        &self.metadata
    }

    /// Coefficients `(C̄_nm, S̄_nm)` for a valid degree and order.
    pub fn coefficient(&self, n: u32, m: u32) -> Option<(f64, f64)> {
        if n < 2 || n > self.degree || m > n {
            return None;
        }
        let index = degree_offset(n) + m as usize;
        Some((
            self.coefficients[2 * index],
            self.coefficients[2 * index + 1],
        ))
    }
}

/// Gravitational acceleration from the `n >= 2` harmonic terms, in m/s².
///
/// `position_body_fixed_m` and the resulting vector are in the same IAU
/// body-fixed axes as the coefficients. Callers rotate through the shared
/// [`crate::domain::services::reference_frames`] helpers and add the existing
/// point-mass term; this function intentionally excludes both.
pub fn spherical_harmonic_acceleration(
    mu_m3_s2: f64,
    position_body_fixed_m: DVec3,
    model: &SphericalHarmonicGravityModel,
) -> DVec3 {
    let r_sq = position_body_fixed_m.length_squared();
    if !mu_m3_s2.is_finite() || mu_m3_s2 <= 0.0 || r_sq < 1.0 {
        return DVec3::ZERO;
    }
    let r = r_sq.sqrt();
    let sin_phi = (position_body_fixed_m.z / r).clamp(-1.0, 1.0);
    let cos_phi = (1.0 - sin_phi * sin_phi).max(0.0).sqrt();
    let lambda = position_body_fixed_m.y.atan2(position_body_fixed_m.x);
    let (sin_lambda, cos_lambda) = lambda.sin_cos();
    let ratio = model.reference_radius_m / r;
    let degree = model.degree as usize;

    // One allocation per evaluation holds the cosine/sine of m*lambda and the
    // six Legendre rows. Keeping the rows contiguous also improves locality
    // for the degree-360 field.
    let stride = degree + 1;
    let mut scratch = vec![0.0; 8 * stride];
    let (cos_m_lambda, scratch) = scratch.split_at_mut(stride);
    let (sin_m_lambda, scratch) = scratch.split_at_mut(stride);
    let (mut p_nm2, scratch) = scratch.split_at_mut(stride);
    let (mut p_nm1, scratch) = scratch.split_at_mut(stride);
    let (mut dp_nm2, scratch) = scratch.split_at_mut(stride);
    let (mut dp_nm1, scratch) = scratch.split_at_mut(stride);
    let (mut p_n, mut dp_n) = scratch.split_at_mut(stride);
    cos_m_lambda[0] = 1.0;
    for m in 1..=degree {
        cos_m_lambda[m] = cos_m_lambda[m - 1] * cos_lambda - sin_m_lambda[m - 1] * sin_lambda;
        sin_m_lambda[m] = sin_m_lambda[m - 1] * cos_lambda + cos_m_lambda[m - 1] * sin_lambda;
    }

    // Fully normalized associated Legendre functions and their derivatives
    // with respect to geocentric latitude.
    p_nm2[0] = 1.0;
    let sqrt_three = 3.0_f64.sqrt();
    p_nm1[0] = sqrt_three * sin_phi;
    p_nm1[1] = sqrt_three * cos_phi;
    dp_nm1[0] = sqrt_three * cos_phi;
    dp_nm1[1] = -sqrt_three * sin_phi;

    let mut radial = 0.0;
    let mut latitudinal = 0.0;
    let mut longitudinal = 0.0;
    let mut ratio_power = ratio * ratio;
    let mut row_offset = 0;

    for n in 2..=degree {
        let n_f = n as f64;
        let diagonal = ((2.0 * n_f + 1.0) / (2.0 * n_f)).sqrt();
        p_n[n] = diagonal * cos_phi * p_nm1[n - 1];
        dp_n[n] = diagonal * (-sin_phi * p_nm1[n - 1] + cos_phi * dp_nm1[n - 1]);
        for m in 0..n {
            let m_f = m as f64;
            let a = (((2.0 * n_f + 1.0) * (2.0 * n_f - 1.0)) / ((n_f - m_f) * (n_f + m_f))).sqrt();
            let b = (((2.0 * n_f + 1.0) * (n_f - m_f - 1.0) * (n_f + m_f - 1.0))
                / ((2.0 * n_f - 3.0) * (n_f + m_f) * (n_f - m_f)))
                .sqrt();
            p_n[m] = a * sin_phi * p_nm1[m] - b * p_nm2[m];
            dp_n[m] = a * (cos_phi * p_nm1[m] + sin_phi * dp_nm1[m]) - b * dp_nm2[m];
        }

        let mut sum_r = 0.0;
        let mut sum_phi = 0.0;
        let mut sum_lambda = 0.0;
        for m in 0..=n {
            let (c, s) = (
                model.coefficients[2 * (row_offset + m)],
                model.coefficients[2 * (row_offset + m) + 1],
            );
            let term = c * cos_m_lambda[m] + s * sin_m_lambda[m];
            let p = p_n[m];
            sum_r += p * term;
            sum_phi += dp_n[m] * term;
            sum_lambda += (m as f64) * p * (s * cos_m_lambda[m] - c * sin_m_lambda[m]);
        }
        radial -= (n_f + 1.0) * ratio_power * sum_r;
        latitudinal += ratio_power * sum_phi;
        longitudinal += ratio_power * sum_lambda;

        row_offset += n + 1;
        ratio_power *= ratio;
        std::mem::swap(&mut p_nm2, &mut p_nm1);
        std::mem::swap(&mut p_nm1, &mut p_n);
        std::mem::swap(&mut dp_nm2, &mut dp_nm1);
        std::mem::swap(&mut dp_nm1, &mut dp_n);
    }

    let scale = mu_m3_s2 / r_sq;
    let e_r = position_body_fixed_m / r;
    let e_phi = DVec3::new(-sin_phi * cos_lambda, -sin_phi * sin_lambda, cos_phi);
    let e_lambda = DVec3::new(-sin_lambda, cos_lambda, 0.0);
    // The longitudinal term carries 1/cos(phi); at the exact pole it is
    // undefined and its metric contribution vanishes with cos(phi).
    let longitudinal = if cos_phi > 1.0e-12 {
        longitudinal / cos_phi
    } else {
        0.0
    };
    let acceleration = scale * (radial * e_r + latitudinal * e_phi + longitudinal * e_lambda);
    if acceleration.is_finite() {
        acceleration
    } else {
        DVec3::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::{
        earth_j2_acceleration, gravitational_acceleration_from_mu, EarthJ2GravityModel,
    };

    const MU: f64 = 3.986_004_418e14;
    const RADIUS_M: f64 = 6_378_136.3;

    fn metadata(normalization: &str) -> GravityHarmonicsMetadata {
        GravityHarmonicsMetadata {
            model_id: "test".into(),
            body: "Earth".into(),
            source: "unit test".into(),
            source_url: "https://example.invalid/source".into(),
            source_sha256: "0".repeat(64),
            normalization: normalization.into(),
            tide_system: "tide-free".into(),
            license: "test".into(),
            attribution: "test".into(),
            conversion_version: 1,
        }
    }

    fn j2_degree_two(j2: f64) -> SphericalHarmonicGravityModel {
        let c20 = -j2 / 5.0_f64.sqrt();
        let coefficients = vec![c20, 0.0, 0.0, 0.0, 0.0, 0.0];
        SphericalHarmonicGravityModel::from_parts(
            2,
            RADIUS_M,
            metadata("fully-normalized-4pi"),
            coefficients,
        )
        .unwrap()
    }

    #[test]
    fn degree_two_matches_the_authoritative_j2_filter() {
        let j2 = 1.082_626_173_852_222_7e-3;
        let model = j2_degree_two(j2);
        let j2_model = EarthJ2GravityModel {
            model_id: "EGM2008".into(),
            reference_radius_m: RADIUS_M,
            j2,
        };
        for position in [
            DVec3::new(7_000_000.0, 0.0, 0.0),
            DVec3::new(4_500_000.0, 3_000_000.0, 1_500_000.0),
            DVec3::new(-2_000_000.0, 1_000_000.0, 6_500_000.0),
        ] {
            let expected = gravitational_acceleration_from_mu(MU, position, DVec3::ZERO)
                + earth_j2_acceleration(MU, position, DVec3::Z, &j2_model);
            let actual = gravitational_acceleration_from_mu(MU, position, DVec3::ZERO)
                + spherical_harmonic_acceleration(MU, position, &model);
            assert!(
                (actual - expected).length() < expected.length() * 1.0e-12,
                "degree-two harmonic field disagrees with the J2 authority: {actual} vs {expected}"
            );
        }
    }

    #[test]
    fn zonal_field_is_axisymmetric_about_the_spin_axis() {
        let model = j2_degree_two(1.082_626_173_852_222_7e-3);
        let base = DVec3::new(4_000_000.0, 2_000_000.0, 3_000_000.0);
        // A quarter turn about the spin axis preserves radius and latitude.
        let rotated = DVec3::new(-base.y, base.x, base.z);
        let a = spherical_harmonic_acceleration(MU, base, &model);
        let b = spherical_harmonic_acceleration(MU, rotated, &model);
        assert!((a.length() - b.length()).abs() < a.length() * 1.0e-12);
        assert!((a.z - b.z).abs() < a.z.abs().max(1.0e-12) * 1.0e-12);
    }

    #[test]
    fn empty_field_produces_no_harmonic_acceleration() {
        let model = SphericalHarmonicGravityModel::from_parts(
            2,
            RADIUS_M,
            metadata("fully-normalized-4pi"),
            vec![0.0; 6],
        )
        .unwrap();
        let acceleration =
            spherical_harmonic_acceleration(MU, DVec3::new(7_000_000.0, 0.0, 0.0), &model);
        assert_eq!(acceleration, DVec3::ZERO);
    }

    #[test]
    fn package_round_trips_and_rejects_other_conventions() {
        let model = j2_degree_two(1.0e-3);
        let bytes = model.write_bytes();
        let decoded = SphericalHarmonicGravityModel::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.degree(), 2);
        assert_eq!(decoded.coefficient(2, 0), model.coefficient(2, 0));
        let mut corrupted = bytes.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xFF;
        assert!(SphericalHarmonicGravityModel::from_bytes(&corrupted).is_err());
        assert!(SphericalHarmonicGravityModel::from_parts(
            2,
            RADIUS_M,
            metadata("unnormalized"),
            vec![0.0; 6]
        )
        .is_err());
    }

    #[test]
    fn coefficient_pair_count_matches_the_triangular_layout() {
        assert_eq!(coefficient_pair_count(2), 3);
        assert_eq!(coefficient_pair_count(3), 7);
        assert_eq!(coefficient_pair_count(360), 65_338);
        assert_eq!(degree_offset(2), 0);
        assert_eq!(degree_offset(3), 3);
    }
}

#[cfg(test)]
mod independent_reference {
    use super::*;

    /// Synthetic degree-four field used to cross-check the general m > 0
    /// recursion against an independent SciPy potential gradient.
    fn probe_model() -> SphericalHarmonicGravityModel {
        let coefficients = vec![
            1.0e-5, 0.0, 2.0e-6, -4.0e-6, 3.0e-6, 5.0e-6, -6.0e-6, 0.0, 7.0e-6, -1.0e-6, -8.0e-6,
            2.0e-6, 9.0e-6, -3.0e-6, 1.1e-5, 0.0, -1.2e-6, 1.6e-6, 1.3e-6, -1.7e-6, -1.4e-6,
            1.8e-6, 1.5e-6, -1.9e-6,
        ];
        SphericalHarmonicGravityModel::from_parts(
            4,
            6_378_136.3,
            GravityHarmonicsMetadata {
                model_id: "independent-reference".into(),
                body: "Earth".into(),
                source: "unit test".into(),
                source_url: "https://example.invalid/reference".into(),
                source_sha256: "0".repeat(64),
                normalization: "fully-normalized-4pi".into(),
                tide_system: "tide-free".into(),
                license: "test".into(),
                attribution: "test".into(),
                conversion_version: 1,
            },
            coefficients,
        )
        .unwrap()
    }

    #[test]
    fn general_harmonics_match_an_independent_potential_gradient() {
        // Reference computed outside this crate with SciPy's associated
        // Legendre functions and a 1 m central-difference gradient of the
        // potential at the same position. Agreement is limited by the finite
        // difference, not by the recursion.
        let expected = DVec3::new(
            4.844_507_998_598_146_5e-4,
            -2.175_202_027_387_968e-5,
            4.113_898_410_196_270_6e-5,
        );
        let actual = spherical_harmonic_acceleration(
            3.986_004_418e14,
            DVec3::new(4_500_000.0, 3_000_000.0, 2_000_000.0),
            &probe_model(),
        );
        assert!(
            (actual - expected).length() < expected.length() * 1.0e-8,
            "independent reference mismatch: {actual} vs {expected}"
        );
    }
}
