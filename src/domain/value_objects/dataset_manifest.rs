//! Unified terrain dataset registry and provenance contract.
//!
//! One registry entry describes one authoritative dataset layer: where it came
//! from, how it is licensed, which datum it uses, what runtime asset it produces,
//! and the checksums that prove the asset is the reviewed one. The registry is
//! metadata only; it never loads terrain and it never downloads data at runtime.
//!
//! Existing per-layer manifests remain the owners of their own validation
//! (imagery and elevation pyramids). A registry entry may reference one through
//! `manifest_path`; the preprocess checker parses and validates that manifest
//! instead of duplicating its rules.

use serde::{Deserialize, Serialize};

/// Registry location for Earth's terrain datasets.
pub const DEFAULT_EARTH_DATASET_REGISTRY: &str = "assets/configs/terrain/earth_datasets_v1.ron";

/// Terrain data layer a dataset provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetLayer {
    Dem,
    Imagery,
    LandCover,
    Soil,
    Water,
    Canopy,
    Climate,
}

impl DatasetLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dem => "dem",
            Self::Imagery => "imagery",
            Self::LandCover => "landcover",
            Self::Soil => "soil",
            Self::Water => "water",
            Self::Canopy => "canopy",
            Self::Climate => "climate",
        }
    }

    /// Parse the `--layer` CLI vocabulary; `all` is handled by the caller.
    pub fn from_cli(value: &str) -> Option<Self> {
        match value {
            "dem" => Some(Self::Dem),
            "imagery" => Some(Self::Imagery),
            "landcover" => Some(Self::LandCover),
            "soil" => Some(Self::Soil),
            "water" => Some(Self::Water),
            "canopy" => Some(Self::Canopy),
            "climate" => Some(Self::Climate),
            _ => None,
        }
    }
}

/// Package lifecycle. Only `Ready` packages may be consumed by the simulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetStatus {
    /// Reviewed, generated, checksummed, and usable.
    Ready,
    /// Declared but not yet produced; never accepted.
    Pending,
    /// Required artifact is absent; never accepted.
    Missing,
    /// Superseded but retained for rollback; never accepted by default.
    Retired,
}

/// Coverage of a dataset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatasetBounds {
    Global,
    Region {
        west_deg: f64,
        south_deg: f64,
        east_deg: f64,
        north_deg: f64,
    },
}

impl DatasetBounds {
    pub fn label(&self) -> String {
        match self {
            Self::Global => "global".into(),
            Self::Region {
                west_deg,
                south_deg,
                east_deg,
                north_deg,
            } => format!("{west_deg}..{east_deg}E {south_deg}..{north_deg}N"),
        }
    }
}

/// Additional runtime file that belongs to a dataset but is not its primary
/// height or imagery asset, such as a sea-level geoid grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuxiliaryAsset {
    pub path: String,
    pub sha256: String,
    #[serde(default)]
    pub role: String,
}

/// Provenance and runtime artifact for one terrain dataset layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetEntry {
    /// Stable registry identity, unique within a body.
    pub id: String,
    pub layer: DatasetLayer,
    /// Optional regional identity (for example `papua_coastal_lowland`).
    pub region: Option<String>,
    pub dataset: String,
    pub source_url: String,
    pub source_version: String,
    pub resolution_m: f64,
    pub bounds: DatasetBounds,
    pub horizontal_datum: String,
    pub vertical_datum: String,
    pub license: String,
    pub attribution: String,
    pub source_sha256: String,
    /// Repository-relative runtime asset (a file or a tile directory).
    pub runtime_asset: String,
    /// File SHA-256, or the aggregate checksum for a tile directory.
    pub runtime_sha256: String,
    /// Extra runtime files required for correct interpretation, such as a geoid.
    #[serde(default)]
    pub auxiliary_assets: Vec<AuxiliaryAsset>,
    pub converter: String,
    pub status: DatasetStatus,
    /// Optional existing per-layer manifest that owns deeper validation.
    pub manifest_path: Option<String>,
}

/// A body's complete terrain dataset registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainDatasetRegistry {
    pub body: String,
    pub version: u32,
    pub runtime_download: bool,
    pub datasets: Vec<DatasetEntry>,
}

impl TerrainDatasetRegistry {
    /// Parse a registry without touching the filesystem.
    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|error| format!("invalid dataset registry: {error}"))
    }

    /// Reject incomplete, duplicated, or unverified entries before any asset is
    /// read. Filesystem checks belong to the preprocess checker.
    pub fn validate(&self) -> Result<(), String> {
        if self.body.is_empty() {
            return Err("dataset registry body is required".into());
        }
        if self.version == 0 {
            return Err("dataset registry version must be positive".into());
        }
        if self.runtime_download {
            return Err("terrain datasets must not require runtime downloads".into());
        }
        if self.datasets.is_empty() {
            return Err("dataset registry is empty".into());
        }

        let mut seen = std::collections::HashSet::with_capacity(self.datasets.len());
        for entry in &self.datasets {
            entry.validate()?;
            if !seen.insert(entry.id.as_str()) {
                return Err(format!("duplicate dataset id {}", entry.id));
            }
        }
        Ok(())
    }

    pub fn entry(&self, id: &str) -> Option<&DatasetEntry> {
        self.datasets.iter().find(|entry| entry.id == id)
    }

    /// Entries selected by a layer and optional region filter.
    pub fn selected<'a>(
        &'a self,
        layer: Option<DatasetLayer>,
        region: Option<&'a str>,
    ) -> impl Iterator<Item = &'a DatasetEntry> + 'a {
        self.datasets.iter().filter(move |entry| {
            layer.is_none_or(|layer| entry.layer == layer)
                && region.is_none_or(|region| entry.region.as_deref() == Some(region))
        })
    }
}

impl DatasetEntry {
    pub fn validate(&self) -> Result<(), String> {
        let required = [
            ("id", &self.id),
            ("dataset", &self.dataset),
            ("source_url", &self.source_url),
            ("source_version", &self.source_version),
            ("horizontal_datum", &self.horizontal_datum),
            ("vertical_datum", &self.vertical_datum),
            ("license", &self.license),
            ("attribution", &self.attribution),
            ("runtime_asset", &self.runtime_asset),
            ("converter", &self.converter),
        ];
        for (field, value) in required {
            if value.trim().is_empty() {
                return Err(format!("dataset {} {} is required", self.id, field));
            }
        }
        if self
            .region
            .as_ref()
            .is_some_and(|region| region.trim().is_empty())
        {
            return Err(format!("dataset {} region must not be empty", self.id));
        }
        if !self.resolution_m.is_finite() || self.resolution_m <= 0.0 {
            return Err(format!(
                "dataset {} resolution must be finite and positive",
                self.id
            ));
        }
        self.bounds.validate(&self.id)?;
        if !is_repo_relative(&self.runtime_asset) {
            return Err(format!(
                "dataset {} runtime asset must be repository-relative",
                self.id
            ));
        }
        if let Some(manifest) = &self.manifest_path {
            if !is_repo_relative(manifest) {
                return Err(format!(
                    "dataset {} manifest path must be repository-relative",
                    self.id
                ));
            }
        }
        validate_sha256(&self.id, "source", &self.source_sha256)?;
        validate_sha256(&self.id, "runtime", &self.runtime_sha256)?;
        let mut auxiliary_paths = std::collections::HashSet::new();
        for auxiliary in &self.auxiliary_assets {
            if !is_repo_relative(&auxiliary.path) {
                return Err(format!(
                    "dataset {} auxiliary asset must be repository-relative",
                    self.id
                ));
            }
            if auxiliary.path == self.runtime_asset {
                return Err(format!(
                    "dataset {} auxiliary asset duplicates the runtime asset",
                    self.id
                ));
            }
            if !auxiliary_paths.insert(auxiliary.path.as_str()) {
                return Err(format!(
                    "dataset {} has duplicate auxiliary asset {}",
                    self.id, auxiliary.path
                ));
            }
            validate_sha256(&self.id, "auxiliary", &auxiliary.sha256)?;
        }
        if self.status != DatasetStatus::Ready {
            return Err(format!(
                "dataset {} is {:?}; only Ready datasets are accepted",
                self.id, self.status
            ));
        }
        Ok(())
    }
}

impl DatasetBounds {
    fn validate(&self, id: &str) -> Result<(), String> {
        let Self::Region {
            west_deg,
            south_deg,
            east_deg,
            north_deg,
        } = self
        else {
            return Ok(());
        };
        if !(west_deg.is_finite()
            && south_deg.is_finite()
            && east_deg.is_finite()
            && north_deg.is_finite())
        {
            return Err(format!("dataset {id} bounds must be finite"));
        }
        if !(west_deg < east_deg && south_deg < north_deg) {
            return Err(format!("dataset {id} bounds are inverted"));
        }
        if *west_deg < -180.0 || *east_deg > 180.0 || *south_deg < -90.0 || *north_deg > 90.0 {
            return Err(format!("dataset {id} bounds are outside geographic range"));
        }
        Ok(())
    }
}

fn is_repo_relative(path: &str) -> bool {
    !path.trim().is_empty()
        && !path.starts_with('/')
        && !path.split('/').any(|part| part == "..")
        && !path.contains('\\')
}

fn validate_sha256(id: &str, label: &str, value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "dataset {id} {label} checksum must be a verified 64-character SHA-256"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKED_IN: &str = include_str!("../../../assets/configs/terrain/earth_datasets_v1.ron");

    fn registry() -> TerrainDatasetRegistry {
        let registry = TerrainDatasetRegistry::from_ron(CHECKED_IN)
            .expect("the checked-in registry must be valid RON");
        registry
            .validate()
            .expect("the checked-in registry must be fully verified");
        registry
    }

    #[test]
    fn checked_in_registry_declares_the_active_earth_layers() {
        let registry = registry();
        assert_eq!(registry.body, "Earth");
        assert!(!registry.runtime_download);
        let dem = registry
            .entry("earth_etopo1_ice_surface")
            .expect("ETOPO1 entry");
        assert_eq!(dem.layer, DatasetLayer::Dem);
        assert_eq!(dem.bounds, DatasetBounds::Global);
        assert_eq!(
            dem.runtime_asset,
            "assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem"
        );
        let etopo2022 = registry
            .entry("earth_etopo2022_30s_ice_surface")
            .expect("ETOPO 2022 entry");
        assert_eq!(etopo2022.auxiliary_assets.len(), 1);
        assert_eq!(
            etopo2022.auxiliary_assets[0].path,
            "assets/large_files/terrain/earth_egm2008_geoid_0p5deg_i16cm.raw"
        );
        let imagery = registry
            .entry("earth_imagery_sentinel2_papua")
            .expect("Sentinel-2 entry");
        assert!(matches!(imagery.bounds, DatasetBounds::Region { .. }));
        assert_eq!(imagery.region.as_deref(), Some("papua_coastal_lowland"));
    }

    #[test]
    fn auxiliary_checksums_are_validated() {
        let mut registry = registry();
        let entry = registry
            .datasets
            .iter_mut()
            .find(|entry| entry.id == "earth_etopo2022_30s_ice_surface")
            .expect("ETOPO 2022 entry");
        entry.auxiliary_assets[0].sha256 = "pending".into();
        let error = registry
            .validate()
            .expect_err("pending auxiliary checksums must be rejected");
        assert!(error.contains("auxiliary"), "unexpected error: {error}");
    }

    #[test]
    fn pending_checksums_are_rejected() {
        let mut registry = registry();
        registry.datasets[0].runtime_sha256 = "pending".into();
        let error = registry
            .validate()
            .expect_err("pending checksums must be rejected");
        assert!(error.contains("checksum"), "unexpected error: {error}");
    }

    #[test]
    fn non_ready_entries_are_rejected() {
        let mut registry = registry();
        registry.datasets[0].status = DatasetStatus::Pending;
        let error = registry
            .validate()
            .expect_err("pending datasets must be rejected");
        assert!(error.contains("only Ready"), "unexpected error: {error}");
    }

    #[test]
    fn runtime_download_registries_are_rejected() {
        let mut registry = registry();
        registry.runtime_download = true;
        let error = registry
            .validate()
            .expect_err("runtime downloads must be rejected");
        assert!(error.contains("runtime downloads"), "unexpected: {error}");
    }

    #[test]
    fn duplicate_ids_and_inverted_bounds_are_rejected() {
        let mut duplicates = registry();
        let duplicate = duplicates.datasets[0].clone();
        duplicates.datasets.push(duplicate);
        assert!(duplicates.validate().is_err());

        let mut inverted = registry();
        inverted.datasets[1].bounds = DatasetBounds::Region {
            west_deg: 10.0,
            south_deg: 0.0,
            east_deg: 5.0,
            north_deg: 1.0,
        };
        let error = inverted
            .validate()
            .expect_err("inverted bounds must be rejected");
        assert!(error.contains("inverted"), "unexpected error: {error}");
    }

    #[test]
    fn layer_and_region_filters_select_ready_entries() {
        let registry = registry();
        let dems: Vec<_> = registry.selected(Some(DatasetLayer::Dem), None).collect();
        assert_eq!(dems.len(), 6);
        assert!(dems
            .iter()
            .any(|entry| entry.id == "earth_etopo1_ice_surface"));
        assert!(dems
            .iter()
            .any(|entry| entry.id == "earth_etopo2022_30s_ice_surface"));
        assert!(dems
            .iter()
            .any(|entry| entry.id == "earth_regional_dem_papua"));
        assert!(dems
            .iter()
            .any(|entry| entry.id == "earth_regional_dem_ksc"));
        assert!(dems
            .iter()
            .any(|entry| entry.id == "earth_regional_dem_starbase"));
        assert!(dems
            .iter()
            .any(|entry| entry.id == "earth_regional_dem_vandenberg"));
        let papua: Vec<_> = registry
            .selected(None, Some("papua_coastal_lowland"))
            .collect();
        assert_eq!(papua.len(), 2);
        assert!(papua
            .iter()
            .any(|entry| entry.id == "earth_imagery_sentinel2_papua"));
        assert!(papua
            .iter()
            .any(|entry| entry.id == "earth_regional_dem_papua"));
        let ksc: Vec<_> = registry.selected(None, Some("ksc_florida")).collect();
        assert_eq!(ksc.len(), 1);
        assert_eq!(ksc[0].id, "earth_regional_dem_ksc");
        let starbase: Vec<_> = registry
            .selected(None, Some("starbase_boca_chica"))
            .collect();
        assert_eq!(starbase.len(), 1);
        assert_eq!(starbase[0].id, "earth_regional_dem_starbase");
        let vandenberg: Vec<_> = registry
            .selected(None, Some("vandenberg_california"))
            .collect();
        assert_eq!(vandenberg.len(), 1);
        assert_eq!(vandenberg[0].id, "earth_regional_dem_vandenberg");
    }

    #[test]
    fn absolute_asset_paths_are_rejected() {
        let mut registry = registry();
        registry.datasets[0].runtime_asset = "/tmp/etopo1.csdem".into();
        let error = registry
            .validate()
            .expect_err("absolute runtime paths must be rejected");
        assert!(error.contains("repository-relative"), "unexpected: {error}");
    }
}
