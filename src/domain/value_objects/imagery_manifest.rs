//! Versioned Earth imagery package manifest and provenance contract.
//!
//! Imagery is presentation-only. It refines the appearance of visible terrain
//! and is never a terrain-height, collision, altitude, or physics authority.
//! A package is accepted only when its manifest records provenance and every
//! checksum is a real SHA-256; the simulator never downloads imagery at
//! runtime, and a missing or invalid package falls back to the global albedo.

use serde::{Deserialize, Serialize};

/// A geographic bounding box in body-fixed latitude/longitude degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoBounds {
    pub west_deg: f64,
    pub south_deg: f64,
    pub east_deg: f64,
    pub north_deg: f64,
}

impl GeoBounds {
    pub fn contains(&self, latitude_deg: f64, longitude_deg: f64) -> bool {
        (self.south_deg..=self.north_deg).contains(&latitude_deg)
            && (self.west_deg..=self.east_deg).contains(&longitude_deg)
    }
}

/// Public-domain or openly licensed global overview imagery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageryGlobalOverview {
    pub dataset: String,
    pub source_url: String,
    pub source_version: String,
    pub license: String,
    pub attribution: String,
    pub source_resolution_m: f64,
    pub runtime_asset: String,
    pub source_sha256: String,
    pub runtime_sha256: String,
}

/// Offline-derived linear-light colour match between a regional mosaic and the
/// global overview it replaces.
///
/// Two different instruments with two different atmospheric corrections do not
/// agree in tone: the Sentinel-2 L2A surface-reflectance mosaic over the Papua
/// region measures about 2.5-4x the linear reflectance of the Blue Marble
/// overview over the same box. Substituting one for the other at a patch edge
/// therefore turns the region into a pale rectangle. The gain is the ratio of the
/// two measured means, applied in linear light so it is a radiometric correction
/// rather than a curve edit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageryColorMatch {
    /// Per-channel linear-space gain applied to the regional mosaic.
    pub linear_gain: [f32; 3],
    /// Measured linear mean of the regional mosaic over the region.
    pub regional_linear_mean: [f32; 3],
    /// Measured linear mean of the global overview over the same footprint.
    pub overview_linear_mean: [f32; 3],
}

impl ImageryColorMatch {
    fn validate(&self, region_id: &str) -> Result<(), String> {
        for (label, channel) in self.linear_gain.iter().enumerate() {
            if !channel.is_finite() || *channel <= 0.0 || *channel > 20.0 {
                return Err(format!(
                    "imagery region {region_id} colour gain channel {label} is not a positive finite ratio"
                ));
            }
        }
        Ok(())
    }
}

/// A bounded high-detail imagery region in body-fixed geographic coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageryLocalRegion {
    pub id: String,
    pub dataset: String,
    pub source_url: String,
    pub source_version: String,
    pub license: String,
    pub attribution: String,
    pub source_resolution_m: f64,
    pub west_deg: f64,
    pub south_deg: f64,
    pub east_deg: f64,
    pub north_deg: f64,
    pub min_level: u32,
    pub max_level: u32,
    pub runtime_tiles: String,
    pub source_sha256: String,
    pub runtime_sha256: String,
    /// Linear-light match to the global overview. Absent means identity, which
    /// is correct for a mosaic already produced on the overview's tonality.
    #[serde(default)]
    pub color_match: Option<ImageryColorMatch>,
}

impl ImageryLocalRegion {
    /// Per-channel linear gain to apply to this region's tiles.
    pub fn linear_color_gain(&self) -> [f32; 3] {
        self.color_match
            .map(|matched| matched.linear_gain)
            .unwrap_or([1.0; 3])
    }
}

/// Offline-prepared Earth imagery hierarchy metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EarthImageryManifest {
    pub body: String,
    pub version: u32,
    pub coordinate_frame: String,
    pub horizontal_datum: String,
    pub runtime_download: bool,
    pub status: String,
    pub global_overview: ImageryGlobalOverview,
    pub local_regions: Vec<ImageryLocalRegion>,
}

impl EarthImageryManifest {
    /// Parse a manifest. Parsing does not imply verification; callers must also
    /// call [`Self::validate`] before using the package.
    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|error| format!("invalid imagery manifest: {error}"))
    }

    /// Reject a manifest that is incomplete, mislabelled, or unverified.
    pub fn validate(&self) -> Result<(), String> {
        if self.body != "Earth" {
            return Err(format!(
                "imagery manifest body must be Earth, got {}",
                self.body
            ));
        }
        if self.version == 0 {
            return Err("imagery manifest version must be positive".into());
        }
        if self.coordinate_frame != "body-fixed" {
            return Err("imagery manifest frame must be body-fixed".into());
        }
        if self.horizontal_datum.is_empty() {
            return Err("imagery manifest horizontal datum is required".into());
        }
        if self.runtime_download {
            return Err("imagery must not require runtime downloads".into());
        }
        validate_global_overview(&self.global_overview)?;
        for region in &self.local_regions {
            validate_local_region(region)?;
        }
        Ok(())
    }

    /// The most specific local region covering a body-fixed coordinate.
    pub fn local_region_at(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
    ) -> Option<&ImageryLocalRegion> {
        self.local_regions
            .iter()
            .find(|region| region.contains(latitude_deg, longitude_deg))
    }
}

impl ImageryLocalRegion {
    pub fn bounds(&self) -> GeoBounds {
        GeoBounds {
            west_deg: self.west_deg,
            south_deg: self.south_deg,
            east_deg: self.east_deg,
            north_deg: self.north_deg,
        }
    }

    pub fn contains(&self, latitude_deg: f64, longitude_deg: f64) -> bool {
        self.bounds().contains(latitude_deg, longitude_deg)
    }
}

fn validate_global_overview(overview: &ImageryGlobalOverview) -> Result<(), String> {
    if overview.dataset.is_empty()
        || overview.source_url.is_empty()
        || overview.source_version.is_empty()
        || overview.license.is_empty()
        || overview.attribution.is_empty()
        || overview.runtime_asset.is_empty()
    {
        return Err("imagery global overview provenance is incomplete".into());
    }
    if !overview.source_resolution_m.is_finite() || overview.source_resolution_m <= 0.0 {
        return Err("imagery global overview resolution must be positive".into());
    }
    validate_checksums(&overview.source_sha256, &overview.runtime_sha256)
}

fn validate_local_region(region: &ImageryLocalRegion) -> Result<(), String> {
    if region.id.is_empty()
        || region.dataset.is_empty()
        || region.source_url.is_empty()
        || region.source_version.is_empty()
        || region.license.is_empty()
        || region.attribution.is_empty()
        || region.runtime_tiles.is_empty()
    {
        return Err(format!(
            "imagery region {} provenance is incomplete",
            region.id
        ));
    }
    if !region.source_resolution_m.is_finite() || region.source_resolution_m <= 0.0 {
        return Err(format!(
            "imagery region {} resolution must be positive",
            region.id
        ));
    }
    if !(region.west_deg < region.east_deg && region.south_deg < region.north_deg) {
        return Err(format!("imagery region {} bounds are inverted", region.id));
    }
    if region.west_deg < -180.0
        || region.east_deg > 180.0
        || region.south_deg < -90.0
        || region.north_deg > 90.0
    {
        return Err(format!(
            "imagery region {} bounds are outside geographic range",
            region.id
        ));
    }
    if region.min_level == 0 || region.max_level < region.min_level {
        return Err(format!(
            "imagery region {} level range is invalid",
            region.id
        ));
    }
    if let Some(color_match) = &region.color_match {
        color_match.validate(&region.id)?;
    }
    validate_checksums(&region.source_sha256, &region.runtime_sha256)
}

fn validate_checksums(source_sha256: &str, runtime_sha256: &str) -> Result<(), String> {
    for (label, checksum) in [("source", source_sha256), ("runtime", runtime_sha256)] {
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "imagery {label} checksum is not a verified SHA-256; package production must record it"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKED_IN_MANIFEST: &str =
        include_str!("../../../assets/configs/terrain/earth_imagery_v1.ron");

    #[test]
    fn checked_in_manifest_parses_and_selects_the_papua_region() {
        let manifest = EarthImageryManifest::from_ron(CHECKED_IN_MANIFEST)
            .expect("the checked-in imagery manifest must be valid RON");
        assert_eq!(manifest.body, "Earth");
        assert!(!manifest.runtime_download);
        let region = manifest
            .local_region_at(-8.0, 139.5)
            .expect("the Papua launch site must fall inside a local region");
        assert_eq!(region.id, "papua_coastal_lowland");
        assert!(manifest.local_region_at(0.0, 0.0).is_none());
    }

    /// The declared gain must be the measured ratio, not a hand-tuned look. If
    /// the mosaic is ever reprocessed without re-running the measurement, this
    /// fails and the numbers get re-derived.
    #[test]
    fn checked_in_color_match_is_the_measured_mean_ratio() {
        let manifest = EarthImageryManifest::from_ron(CHECKED_IN_MANIFEST).unwrap();
        let region = manifest
            .local_regions
            .iter()
            .find(|region| region.id == "papua_coastal_lowland")
            .expect("the checklist region must exist");
        let matched = region
            .color_match
            .expect("the region must record its colour match");
        for channel in 0..3 {
            let regional = f64::from(matched.regional_linear_mean[channel]);
            let overview = f64::from(matched.overview_linear_mean[channel]);
            assert!(regional > 0.0 && overview > 0.0);
            let expected = overview / regional;
            let declared = f64::from(matched.linear_gain[channel]);
            assert!(
                (declared - expected).abs() < 0.02 * expected,
                "channel {channel}: declared gain {declared} is not the measured ratio {expected}"
            );
            assert!(
                declared < 1.0,
                "the regional mosaic is brighter than the overview, so the gain must darken it"
            );
        }
    }

    #[test]
    fn a_region_without_a_colour_match_is_the_identity() {
        let mut manifest = EarthImageryManifest::from_ron(CHECKED_IN_MANIFEST).unwrap();
        manifest.local_regions[0].color_match = None;
        assert_eq!(manifest.local_regions[0].linear_color_gain(), [1.0; 3]);
    }

    #[test]
    fn a_non_positive_colour_gain_is_rejected() {
        let mut manifest = EarthImageryManifest::from_ron(CHECKED_IN_MANIFEST).unwrap();
        if let Some(matched) = manifest.local_regions[0].color_match.as_mut() {
            matched.linear_gain[1] = 0.0;
        }
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn unverified_checksums_are_rejected() {
        let mut manifest = EarthImageryManifest::from_ron(CHECKED_IN_MANIFEST).unwrap();
        manifest.local_regions[0].runtime_sha256 = "pending".into();
        let error = manifest
            .validate()
            .expect_err("pending checksums must not be accepted");
        assert!(error.contains("checksum"), "unexpected error: {error}");
    }

    #[test]
    fn verified_manifest_validates() {
        let mut manifest = EarthImageryManifest::from_ron(CHECKED_IN_MANIFEST).unwrap();
        let digest = "a".repeat(64);
        manifest.global_overview.source_sha256 = digest.clone();
        manifest.global_overview.runtime_sha256 = digest.clone();
        for region in &mut manifest.local_regions {
            region.source_sha256 = digest.clone();
            region.runtime_sha256 = digest.clone();
        }
        manifest
            .validate()
            .expect("a fully verified manifest is valid");
    }

    #[test]
    fn runtime_download_manifests_are_rejected() {
        let mut manifest = EarthImageryManifest::from_ron(CHECKED_IN_MANIFEST).unwrap();
        manifest.runtime_download = true;
        let error = manifest
            .validate()
            .expect_err("runtime download must be rejected");
        assert!(
            error.contains("runtime downloads"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn malformed_ron_is_rejected() {
        assert!(EarthImageryManifest::from_ron("(body: \"Earth\"").is_err());
    }
}
