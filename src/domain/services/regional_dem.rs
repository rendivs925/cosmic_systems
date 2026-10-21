//! Regional, high-resolution DEM ingest for the open-science terrain plan.
//!
//! The runtime consumes sparse cube-sphere elevation payload tiles; this module
//! owns the pure source sampling used to bake them. A regional package is
//! anchored to the same WGS 84 ellipsoid as the global authority by adding the
//! EGM2008 geoid undulation to each source sample.

use super::geoid::GeoidModel;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use tiff::decoder::{Decoder, DecodingResult};

/// One reviewed region definition in `assets/configs/terrain/regions_v1.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RegionalDemConfig {
    pub version: u32,
    pub regions: Vec<RegionDefinition>,
}

/// Geographic bounds for a regional package.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct RegionBounds {
    pub west_deg: f64,
    pub south_deg: f64,
    pub east_deg: f64,
    pub north_deg: f64,
}

/// One source raster tile. NASADEM is north-up WGS 84 geographic; `north_deg`
/// and `west_deg` locate the upper-left pixel corner.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RegionalSource {
    pub path: String,
    pub sha256: String,
    pub west_deg: f64,
    pub north_deg: f64,
    pub pixel_deg: f64,
}

/// A reviewed regional package definition.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RegionDefinition {
    pub id: String,
    pub body: String,
    pub bounds: RegionBounds,
    pub min_level: u32,
    pub max_level: u32,
    pub tile_resolution: u32,
    pub geoid_grid: String,
    pub source_name: String,
    pub license: String,
    pub vertical_datum: String,
    pub resolution_m: f64,
    pub sources: Vec<RegionalSource>,
}

#[derive(Debug)]
pub enum RegionalDemError {
    Io(std::io::Error),
    Image(String),
    Format(String),
    Geoid(String),
}

impl std::fmt::Display for RegionalDemError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Image(message) => write!(formatter, "cannot decode regional source: {message}"),
            Self::Format(message) => write!(formatter, "invalid regional source: {message}"),
            Self::Geoid(message) => write!(formatter, "cannot load regional geoid: {message}"),
        }
    }
}

impl std::error::Error for RegionalDemError {}

impl From<std::io::Error> for RegionalDemError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl RegionalDemConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RegionalDemError> {
        let text = fs::read_to_string(path)?;
        let config: Self = ron::from_str(&text)
            .map_err(|error| RegionalDemError::Format(format!("invalid RON: {error}")))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), RegionalDemError> {
        if self.version != 1 {
            return Err(RegionalDemError::Format(
                "regional terrain config version must be 1".into(),
            ));
        }
        if self.regions.is_empty() {
            return Err(RegionalDemError::Format(
                "regional terrain config has no regions".into(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for region in &self.regions {
            region.validate()?;
            if !ids.insert(region.id.as_str()) {
                return Err(RegionalDemError::Format(format!(
                    "duplicate regional terrain id {}",
                    region.id
                )));
            }
        }
        Ok(())
    }

    pub fn region(&self, id: &str) -> Option<&RegionDefinition> {
        self.regions.iter().find(|region| region.id == id)
    }
}

impl RegionDefinition {
    fn validate(&self) -> Result<(), RegionalDemError> {
        for (field, value) in [
            ("id", &self.id),
            ("body", &self.body),
            ("geoid_grid", &self.geoid_grid),
            ("source_name", &self.source_name),
            ("license", &self.license),
            ("vertical_datum", &self.vertical_datum),
        ] {
            if value.trim().is_empty() {
                return Err(RegionalDemError::Format(format!(
                    "region {} {field} is required",
                    self.id
                )));
            }
        }
        if !is_repo_relative(&self.geoid_grid) {
            return Err(RegionalDemError::Format(format!(
                "region {} geoid grid must be repository-relative",
                self.id
            )));
        }
        self.bounds.validate(&self.id)?;
        if self.min_level > self.max_level || self.max_level > 20 {
            return Err(RegionalDemError::Format(format!(
                "region {} payload levels are invalid",
                self.id
            )));
        }
        if !(2..=4096).contains(&self.tile_resolution) {
            return Err(RegionalDemError::Format(format!(
                "region {} tile resolution must be 2..=4096",
                self.id
            )));
        }
        if !self.resolution_m.is_finite() || self.resolution_m <= 0.0 {
            return Err(RegionalDemError::Format(format!(
                "region {} resolution must be finite and positive",
                self.id
            )));
        }
        if self.sources.is_empty() {
            return Err(RegionalDemError::Format(format!(
                "region {} has no source tiles",
                self.id
            )));
        }
        for source in &self.sources {
            if !is_repo_relative(&source.path) {
                return Err(RegionalDemError::Format(format!(
                    "region {} source must be repository-relative",
                    self.id
                )));
            }
            if source.sha256.len() != 64
                || !source.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(RegionalDemError::Format(format!(
                    "region {} source checksum is invalid",
                    self.id
                )));
            }
            if !source.west_deg.is_finite()
                || !source.north_deg.is_finite()
                || !source.pixel_deg.is_finite()
                || source.pixel_deg <= 0.0
            {
                return Err(RegionalDemError::Format(format!(
                    "region {} source georeferencing is invalid",
                    self.id
                )));
            }
        }
        Ok(())
    }
}

impl RegionBounds {
    fn validate(&self, id: &str) -> Result<(), RegionalDemError> {
        if !(self.west_deg.is_finite()
            && self.south_deg.is_finite()
            && self.east_deg.is_finite()
            && self.north_deg.is_finite())
        {
            return Err(RegionalDemError::Format(format!(
                "region {id} bounds must be finite"
            )));
        }
        if !(self.west_deg < self.east_deg && self.south_deg < self.north_deg) {
            return Err(RegionalDemError::Format(format!(
                "region {id} bounds are inverted"
            )));
        }
        if self.west_deg < -180.0
            || self.east_deg > 180.0
            || self.south_deg < -90.0
            || self.north_deg > 90.0
        {
            return Err(RegionalDemError::Format(format!(
                "region {id} bounds are outside geographic range"
            )));
        }
        Ok(())
    }
}

/// Loaded regional sources plus the shared sea-level geoid.
pub struct RegionalDem {
    tiles: Vec<LoadedTile>,
    geoid: GeoidModel,
    source_sha256: String,
}

struct LoadedTile {
    west_deg: f64,
    north_deg: f64,
    pixel_deg: f64,
    width: u32,
    height: u32,
    heights_m: Vec<i16>,
}

impl RegionalDem {
    pub fn load(
        region: RegionDefinition,
        root: impl AsRef<Path>,
        geoid: GeoidModel,
    ) -> Result<Self, RegionalDemError> {
        let root = root.as_ref();
        let mut digest = Sha256::new();
        let mut tiles = Vec::with_capacity(region.sources.len());
        for source in &region.sources {
            let bytes = fs::read(root.join(&source.path))?;
            let found = hex_sha256(&bytes);
            if !found.eq_ignore_ascii_case(&source.sha256) {
                return Err(RegionalDemError::Format(format!(
                    "source {} checksum mismatch: expected {}, found {found}",
                    source.path, source.sha256
                )));
            }
            digest.update(&bytes);
            tiles.push(LoadedTile::decode(source, &bytes)?);
        }
        Ok(Self {
            tiles,
            geoid,
            source_sha256: format!("{:x}", digest.finalize()),
        })
    }

    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    /// Ellipsoidal height at a geographic position, or `None` outside all source
    /// tiles. NASADEM stores EGM96 orthometric heights; adding the local EGM2008
    /// undulation expresses them in the runtime's WGS 84 ellipsoid frame.
    pub fn height_ellipsoid_m(&self, latitude_deg: f64, longitude_deg: f64) -> Option<f64> {
        let orthometric_m = self
            .tiles
            .iter()
            .find_map(|tile| tile.sample_m(latitude_deg, longitude_deg))?;
        Some(orthometric_m + self.geoid.undulation_m(latitude_deg, longitude_deg))
    }
}

impl LoadedTile {
    fn decode(source: &RegionalSource, bytes: &[u8]) -> Result<Self, RegionalDemError> {
        let mut decoder = Decoder::new(Cursor::new(bytes))
            .map_err(|error| RegionalDemError::Image(error.to_string()))?;
        let (width, height) = decoder
            .dimensions()
            .map_err(|error| RegionalDemError::Image(error.to_string()))?;
        if width < 2 || height < 2 {
            return Err(RegionalDemError::Image(
                "source tile is smaller than two samples".into(),
            ));
        }
        let heights_m = match decoder
            .read_image()
            .map_err(|error| RegionalDemError::Image(error.to_string()))?
        {
            DecodingResult::I16(samples) => samples,
            _ => {
                return Err(RegionalDemError::Image(
                    "expected signed 16-bit integer samples".into(),
                ))
            }
        };
        if heights_m.len() != width as usize * height as usize {
            return Err(RegionalDemError::Image(
                "source tile sample count does not match its dimensions".into(),
            ));
        }
        Ok(Self {
            west_deg: source.west_deg,
            north_deg: source.north_deg,
            pixel_deg: source.pixel_deg,
            width,
            height,
            heights_m,
        })
    }

    fn sample_m(&self, latitude_deg: f64, longitude_deg: f64) -> Option<f64> {
        if !latitude_deg.is_finite() || !longitude_deg.is_finite() {
            return None;
        }
        let x = (longitude_deg - self.west_deg) / self.pixel_deg - 0.5;
        let y = (self.north_deg - latitude_deg) / self.pixel_deg - 0.5;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        // Regional edge samples extend the nearest source column/row so a tile
        // that straddles the reviewed boundary has no nodata seam.
        let x = x.clamp(0.0, (self.width - 1) as f64);
        let y = y.clamp(0.0, (self.height - 1) as f64);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width as usize - 1);
        let y1 = (y0 + 1).min(self.height as usize - 1);
        let at = |x: usize, y: usize| f64::from(self.heights_m[y * self.width as usize + x]);
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        let tx = x - x0 as f64;
        let ty = y - y0 as f64;
        Some((a + (b - a) * tx) * (1.0 - ty) + (c + (d - c) * tx) * ty)
    }
}

fn is_repo_relative(path: &str) -> bool {
    !path.trim().is_empty()
        && !path.starts_with('/')
        && !path.split('/').any(|part| part == "..")
        && !path.contains('\\')
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKED_IN_CONFIG: &str = include_str!("../../../assets/configs/terrain/regions_v1.ron");

    #[test]
    fn checked_in_config_declares_the_papua_sources() {
        let config: RegionalDemConfig =
            ron::from_str(CHECKED_IN_CONFIG).expect("checked-in region config must parse");
        config
            .validate()
            .expect("checked-in region config must validate");
        let region = config
            .region("papua_coastal_lowland")
            .expect("Papua region");
        assert_eq!(region.bounds.west_deg, 139.0);
        assert_eq!(region.bounds.south_deg, -8.6);
        assert_eq!(region.sources.len(), 2);
        assert_eq!(region.min_level, 9);
        assert_eq!(region.max_level, 12);

        let ksc = config.region("ksc_florida").expect("KSC region");
        assert_eq!(ksc.bounds.west_deg, -80.85);
        assert_eq!(ksc.bounds.south_deg, 28.40);
        assert_eq!(ksc.sources.len(), 1);
        assert_eq!(ksc.sources[0].north_deg, 29.0);

        let starbase = config
            .region("starbase_boca_chica")
            .expect("Starbase region");
        assert_eq!(starbase.bounds.west_deg, -97.30);
        assert_eq!(starbase.sources[0].north_deg, 26.0);

        let vandenberg = config
            .region("vandenberg_california")
            .expect("Vandenberg region");
        assert_eq!(vandenberg.bounds.west_deg, -120.75);
        assert_eq!(vandenberg.sources[0].north_deg, 35.0);

        let wallops = config.region("wallops_virginia").expect("Wallops region");
        assert_eq!(wallops.bounds.west_deg, -75.70);
        assert_eq!(wallops.sources[0].north_deg, 38.0);

        let kourou = config
            .region("kourou_french_guiana")
            .expect("Kourou region");
        assert_eq!(kourou.bounds.west_deg, -52.95);
        assert_eq!(kourou.sources[0].north_deg, 6.0);
    }

    #[test]
    fn regional_sampling_is_bilinear_and_clamps_at_the_edge() {
        let tile = LoadedTile {
            west_deg: 0.0,
            north_deg: 1.0,
            pixel_deg: 1.0,
            width: 3,
            height: 3,
            heights_m: vec![0, 10, 20, 30, 40, 50, 60, 70, 80],
        };
        assert_eq!(tile.sample_m(0.5, 0.5), Some(0.0));
        assert_eq!(tile.sample_m(1.0, 1.0), Some(5.0));
        assert_eq!(tile.sample_m(-90.0, 90.0), Some(80.0));
    }

    #[test]
    fn rejects_inverted_region_bounds() {
        let region = RegionDefinition {
            id: "test".into(),
            body: "Earth".into(),
            bounds: RegionBounds {
                west_deg: 10.0,
                south_deg: 2.0,
                east_deg: 9.0,
                north_deg: 1.0,
            },
            min_level: 1,
            max_level: 2,
            tile_resolution: 64,
            geoid_grid: "geoid.raw".into(),
            source_name: "source".into(),
            license: "public domain".into(),
            vertical_datum: "WGS 84".into(),
            resolution_m: 30.0,
            sources: vec![],
        };
        assert!(region.validate().is_err());
    }
}
