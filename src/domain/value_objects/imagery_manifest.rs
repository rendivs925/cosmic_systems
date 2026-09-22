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
