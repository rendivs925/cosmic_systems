//! Offline-prepared cube-sphere imagery package metadata and conversion.
//!
//! Imagery is presentation-only. It deliberately shares [`TerrainPatch`] keys
//! with geometry, while height, collision, and physics remain owned by
//! [`TerrainSource`](crate::domain::services::terrain_source::TerrainSource).

use crate::domain::math::DVec3;
use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, CubeFace, TerrainPatch,
};
use image::{ImageBuffer, Rgba, RgbaImage};
use serde::Deserialize;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

pub const EARTH_IMAGERY_MANIFEST_PATH: &str = "configs/terrain/earth_imagery_v1.ron";
pub const EARTH_IMAGERY_PACKAGE_VERSION: u32 = 1;

/// The best available package tile for a terrain patch and the UV transform
/// from the patch-local mesh coordinates into that tile. UVs outside `[0, 1]`
/// intentionally preserve the global albedo fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainImageryTile {
    pub asset_path: String,
    pub uv_scale_offset: [f32; 4],
}

/// Geographic bounds for an image whose columns increase eastward and rows
/// increase southward. The source and its bounds are presentation-only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeographicBounds {
    pub west_longitude_deg: f64,
    pub south_latitude_deg: f64,
    pub east_longitude_deg: f64,
    pub north_latitude_deg: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TerrainImageryManifest {
    pub package_version: u32,
    pub body: String,
    pub body_fixed_frame: String,
    pub source_name: String,
    pub source_version: String,
    pub license: String,
    pub source_sha256: String,
    pub tile_root: String,
    pub tile_pixels: u32,
    pub min_level: u32,
    pub max_level: u32,
    #[serde(default)]
    pub coverage_tiles: Vec<TerrainPatch>,
}

#[derive(Debug)]
pub enum TerrainImageryError {
    Io(std::io::Error),
    Parse(ron::error::SpannedError),
    Image(image::ImageError),
    InvalidManifest(String),
}

impl Display for TerrainImageryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "imagery I/O error: {error}"),
            Self::Parse(error) => write!(formatter, "imagery manifest parse error: {error}"),
            Self::Image(error) => write!(formatter, "imagery conversion error: {error}"),
            Self::InvalidManifest(message) => {
                write!(formatter, "invalid imagery manifest: {message}")
            }
        }
    }
}

impl Error for TerrainImageryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Image(error) => Some(error),
            Self::InvalidManifest(_) => None,
        }
    }
}

impl From<std::io::Error> for TerrainImageryError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<image::ImageError> for TerrainImageryError {
    fn from(error: image::ImageError) -> Self {
        Self::Image(error)
    }
}

impl TerrainImageryManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, TerrainImageryError> {
        let contents = fs::read_to_string(path)?;
        let manifest: Self = ron::from_str(&contents).map_err(TerrainImageryError::Parse)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), TerrainImageryError> {
        if self.package_version != EARTH_IMAGERY_PACKAGE_VERSION {
            return Err(TerrainImageryError::InvalidManifest(format!(
                "unsupported package version {}; expected {EARTH_IMAGERY_PACKAGE_VERSION}",
                self.package_version
            )));
        }
        if self.body != "Earth" || self.body_fixed_frame != "IAU_EARTH" {
            return Err(TerrainImageryError::InvalidManifest(
                "Earth imagery must declare the IAU_EARTH body-fixed frame".into(),
            ));
        }
        if self.tile_pixels < 2 || !self.tile_pixels.is_power_of_two() {
            return Err(TerrainImageryError::InvalidManifest(
                "tile_pixels must be a power of two and at least two".into(),
            ));
        }
        if self.min_level > self.max_level {
            return Err(TerrainImageryError::InvalidManifest(
                "min_level cannot exceed max_level".into(),
            ));
        }
        for patch in &self.coverage_tiles {
            if patch.level > 31
                || patch.tile_x >= 1u32 << patch.level
                || patch.tile_y >= 1u32 << patch.level
            {
                return Err(TerrainImageryError::InvalidManifest(
                    "coverage tile is outside its cube-face level".into(),
                ));
            }
        }
        if self.source_name == "UNCONFIGURED"
            || self.tile_root.is_empty()
            || self.source_name.is_empty()
            || self.source_sha256.len() != 64
        {
            return Err(TerrainImageryError::InvalidManifest(
                "source metadata and tile_root must be populated".into(),
            ));
        }
        Ok(())
    }

    pub fn best_tile_for(&self, patch: TerrainPatch) -> Option<TerrainImageryTile> {
        if !self.coverage_tiles.is_empty() {
            if let Some(coverage) = self
                .coverage_tiles
                .iter()
                .filter(|coverage| coverage.is_ancestor_of(&patch))
                .max_by_key(|coverage| coverage.level)
            {
                return Some(self.tile_for_relation(patch, *coverage));
            }
            // A single launch-site tile may also appear on its generated
            // ancestors, allowing it to replace the root fallback immediately.
            if self.coverage_tiles.len() == 1 && patch.is_ancestor_of(&self.coverage_tiles[0]) {
                return Some(self.tile_for_relation(patch, self.coverage_tiles[0]));
            }
            return None;
        }
        if patch.level < self.min_level {
            return None;
        }
        let tile_level = patch.level.min(self.max_level);
        Some(self.tile_for_relation(
            patch,
            TerrainPatch {
                face: patch.face,
                level: tile_level,
                tile_x: patch.tile_x >> (patch.level - tile_level),
                tile_y: patch.tile_y >> (patch.level - tile_level),
            },
        ))
    }

    fn tile_for_relation(&self, patch: TerrainPatch, tile: TerrainPatch) -> TerrainImageryTile {
        let scale = 2.0_f32.powi(tile.level as i32 - patch.level as i32);
        TerrainImageryTile {
            asset_path: self.tile_path(tile),
            uv_scale_offset: [
                scale,
                scale,
                patch.tile_x as f32 * scale - tile.tile_x as f32,
                patch.tile_y as f32 * scale - tile.tile_y as f32,
            ],
        }
    }

    fn tile_path(&self, patch: TerrainPatch) -> String {
        format!(
            "{}/{}/{}/{}_{}.png",
            self.tile_root.trim_end_matches('/'),
            cube_face_name(patch.face),
            patch.level,
            patch.tile_x,
            patch.tile_y
        )
    }
}

/// Bake one bounded geographic source into a single sparse cube-sphere tile.
/// Pixels outside the source footprint are transparent so the shader retains
/// the global albedo fallback at patch edges.
pub fn convert_geographic_bounds_image(
    source_path: impl AsRef<Path>,
    output_root: impl AsRef<Path>,
    patch: TerrainPatch,
    bounds: GeographicBounds,
    tile_pixels: u32,
) -> Result<(), TerrainImageryError> {
    if tile_pixels < 2 || !tile_pixels.is_power_of_two() {
        return Err(TerrainImageryError::InvalidManifest(
            "tile_pixels must be a power of two and at least two".into(),
        ));
    }
    if !bounds.west_longitude_deg.is_finite()
        || !bounds.south_latitude_deg.is_finite()
        || !bounds.east_longitude_deg.is_finite()
        || !bounds.north_latitude_deg.is_finite()
        || bounds.west_longitude_deg >= bounds.east_longitude_deg
        || bounds.south_latitude_deg >= bounds.north_latitude_deg
        || bounds.west_longitude_deg < -180.0
        || bounds.east_longitude_deg > 180.0
        || bounds.south_latitude_deg < -90.0
        || bounds.north_latitude_deg > 90.0
    {
        return Err(TerrainImageryError::InvalidManifest(
            "geographic bounds must be finite, ordered, and not cross the antimeridian".into(),
        ));
    }
    if patch.level > 31
        || patch.tile_x >= 1u32 << patch.level
        || patch.tile_y >= 1u32 << patch.level
    {
        return Err(TerrainImageryError::InvalidManifest(
            "target patch is outside its cube-face level".into(),
        ));
    }
    let source = image::open(source_path)?.to_rgba8();
    if source.width() < 2 || source.height() < 2 {
        return Err(TerrainImageryError::InvalidManifest(
            "source imagery must be at least 2 by 2 pixels".into(),
        ));
    }
    let tile = bake_geographic_bounds_tile(&source, patch, bounds, tile_pixels);
    let path = output_root
        .as_ref()
        .join(cube_face_name(patch.face))
        .join(patch.level.to_string())
        .join(format!("{}_{}.png", patch.tile_x, patch.tile_y));
    let Some(parent) = path.parent() else {
        return Err(TerrainImageryError::InvalidManifest(
            "tile path has no parent".into(),
        ));
    };
    fs::create_dir_all(parent)?;
    tile.save(path)?;
    Ok(())
}

pub fn cube_face_name(face: CubeFace) -> &'static str {
    match face {
        CubeFace::PosX => "pos_x",
        CubeFace::NegX => "neg_x",
        CubeFace::PosY => "pos_y",
        CubeFace::NegY => "neg_y",
        CubeFace::PosZ => "pos_z",
        CubeFace::NegZ => "neg_z",
    }
}

/// Bake one equirectangular source into deterministic RGBA8 cube-sphere tiles.
/// Source rows run north-to-south and source columns wrap at the antimeridian.
pub fn convert_equirectangular_image(
    source_path: impl AsRef<Path>,
    output_root: impl AsRef<Path>,
    min_level: u32,
    max_level: u32,
    tile_pixels: u32,
) -> Result<(), TerrainImageryError> {
    if min_level > max_level || tile_pixels < 2 || !tile_pixels.is_power_of_two() {
        return Err(TerrainImageryError::InvalidManifest(
            "levels must be ordered and tile_pixels must be a power of two >= 2".into(),
        ));
    }
    let source = image::open(source_path)?.to_rgba8();
    if source.width() < 2 || source.height() < 2 {
        return Err(TerrainImageryError::InvalidManifest(
            "source imagery must be at least 2 by 2 pixels".into(),
        ));
    }
    let output_root = output_root.as_ref();
    for level in min_level..=max_level {
        let width = 1u32 << level;
        for face in CubeFace::ALL {
            for tile_y in 0..width {
                for tile_x in 0..width {
                    let patch = TerrainPatch {
                        face,
                        level,
                        tile_x,
                        tile_y,
                    };
                    let tile = bake_tile(&source, patch, tile_pixels);
                    let path = output_root
                        .join(cube_face_name(face))
                        .join(level.to_string())
                        .join(format!("{tile_x}_{tile_y}.png"));
                    let Some(parent) = path.parent() else {
                        return Err(TerrainImageryError::InvalidManifest(
                            "tile path has no parent".into(),
                        ));
                    };
                    fs::create_dir_all(parent)?;
                    tile.save(path)?;
                }
            }
        }
    }
    Ok(())
}

fn bake_tile(source: &RgbaImage, patch: TerrainPatch, tile_pixels: u32) -> RgbaImage {
    let (u0, v0, u1, v1) = patch.uv_bounds();
    ImageBuffer::from_fn(tile_pixels, tile_pixels, |x, y| {
        let u = u0 + (u1 - u0) * (f64::from(x) + 0.5) / f64::from(tile_pixels);
        let v = v0 + (v1 - v0) * (f64::from(y) + 0.5) / f64::from(tile_pixels);
        sample_equirectangular(source, face_uv_to_direction(patch.face, u, v))
    })
}

fn bake_geographic_bounds_tile(
    source: &RgbaImage,
    patch: TerrainPatch,
    bounds: GeographicBounds,
    tile_pixels: u32,
) -> RgbaImage {
    let (u0, v0, u1, v1) = patch.uv_bounds();
    ImageBuffer::from_fn(tile_pixels, tile_pixels, |x, y| {
        let u = u0 + (u1 - u0) * (f64::from(x) + 0.5) / f64::from(tile_pixels);
        let v = v0 + (v1 - v0) * (f64::from(y) + 0.5) / f64::from(tile_pixels);
        let (latitude_deg, longitude_deg) =
            direction_to_lat_lon(face_uv_to_direction(patch.face, u, v));
        if longitude_deg < bounds.west_longitude_deg
            || longitude_deg > bounds.east_longitude_deg
            || latitude_deg < bounds.south_latitude_deg
            || latitude_deg > bounds.north_latitude_deg
        {
            return Rgba([0, 0, 0, 0]);
        }
        let source_u = (longitude_deg - bounds.west_longitude_deg)
            / (bounds.east_longitude_deg - bounds.west_longitude_deg);
        let source_v = (bounds.north_latitude_deg - latitude_deg)
            / (bounds.north_latitude_deg - bounds.south_latitude_deg);
        let source_x = (source_u * f64::from(source.width() - 1)).round() as u32;
        let source_y = (source_v * f64::from(source.height() - 1)).round() as u32;
        *source.get_pixel(source_x, source_y)
    })
}

fn sample_equirectangular(source: &RgbaImage, direction: DVec3) -> Rgba<u8> {
    let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
    let width = source.width();
    let height = source.height();
    let u = (longitude_deg / 360.0 + 0.5).rem_euclid(1.0);
    let v = (0.5 - latitude_deg / 180.0).clamp(0.0, 1.0);
    let x = (u * f64::from(width)).floor() as u32 % width;
    let y = (v * f64::from(height - 1)).round() as u32;
    *source.get_pixel(x, y)
}

pub fn package_tile_root(
    manifest: &TerrainImageryManifest,
    asset_root: impl AsRef<Path>,
) -> PathBuf {
    asset_root.as_ref().join(&manifest.tile_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::reference_frames::geodetic_to_body_fixed;
    use crate::domain::value_objects::launch_site_coordinates::predefined_sites;

    fn manifest() -> TerrainImageryManifest {
        TerrainImageryManifest {
            package_version: EARTH_IMAGERY_PACKAGE_VERSION,
            body: "Earth".into(),
            body_fixed_frame: "IAU_EARTH".into(),
            source_name: "test".into(),
            source_version: "v1".into(),
            license: "test".into(),
            source_sha256: "0".repeat(64),
            tile_root: "large_files/terrain/earth_imagery_v1/tiles".into(),
            tile_pixels: 256,
            min_level: 0,
            max_level: 12,
            coverage_tiles: Vec::new(),
        }
    }

    #[test]
    fn manifest_uses_stable_cube_patch_paths() {
        let path = manifest()
            .best_tile_for(TerrainPatch::root(CubeFace::PosZ))
            .unwrap()
            .asset_path;
        assert_eq!(
            path,
            "large_files/terrain/earth_imagery_v1/tiles/pos_z/0/0_0.png"
        );
    }

    #[test]
    fn ksc_imagery_tile_matches_the_rocket_terrain_coordinate() {
        let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(EARTH_IMAGERY_MANIFEST_PATH);
        let manifest = TerrainImageryManifest::load(manifest_path).expect("valid Earth imagery");
        let launch_site = predefined_sites::kennedy_space_center();
        let earth = PlanetFactory::create_by_id(&launch_site.planet_id).expect("Earth exists");
        let patch = TerrainPatch::for_direction(
            geodetic_to_body_fixed(&launch_site, &earth).normalize(),
            manifest.max_level,
        );

        assert!(
            manifest.coverage_tiles.contains(&patch),
            "KSC rocket terrain coordinate {patch:?} must use the declared imagery tile"
        );
    }

    #[test]
    fn finer_patch_reuses_its_best_available_parent_tile() {
        let mut manifest = manifest();
        manifest.max_level = 2;
        let tile = manifest
            .best_tile_for(TerrainPatch {
                face: CubeFace::PosZ,
                level: 4,
                tile_x: 11,
                tile_y: 6,
            })
            .unwrap();
        assert_eq!(
            tile.asset_path,
            "large_files/terrain/earth_imagery_v1/tiles/pos_z/2/2_1.png"
        );
        assert_eq!(tile.uv_scale_offset, [0.25, 0.25, 0.75, 0.5]);
    }

    #[test]
    fn sparse_coverage_uses_only_declared_ancestor_tiles() {
        let mut manifest = manifest();
        manifest.coverage_tiles = vec![TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 2,
            tile_y: 1,
        }];
        let child = manifest
            .best_tile_for(TerrainPatch {
                face: CubeFace::PosZ,
                level: 4,
                tile_x: 11,
                tile_y: 6,
            })
            .unwrap();
        assert_eq!(
            child.asset_path,
            "large_files/terrain/earth_imagery_v1/tiles/pos_z/2/2_1.png"
        );
        assert_eq!(child.uv_scale_offset, [0.25, 0.25, 0.75, 0.5]);
        assert!(manifest
            .best_tile_for(TerrainPatch {
                face: CubeFace::PosZ,
                level: 4,
                tile_x: 1,
                tile_y: 1,
            })
            .is_none());
    }

    #[test]
    fn a_single_sparse_tile_is_mapped_onto_its_generated_parent() {
        let mut manifest = manifest();
        manifest.coverage_tiles = vec![TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 2,
            tile_y: 1,
        }];
        let root = manifest
            .best_tile_for(TerrainPatch::root(CubeFace::PosZ))
            .unwrap();
        assert_eq!(
            root.asset_path,
            "large_files/terrain/earth_imagery_v1/tiles/pos_z/2/2_1.png"
        );
        assert_eq!(root.uv_scale_offset, [4.0, 4.0, -2.0, -1.0]);
    }

    #[test]
    fn geographic_converter_keeps_uncovered_pixels_transparent() {
        let source = std::env::temp_dir().join("cosmic_systems_naip_source.png");
        let output = std::env::temp_dir().join("cosmic_systems_naip_tiles");
        ImageBuffer::from_pixel(4, 4, Rgba([12u8, 34, 56, 255]))
            .save(&source)
            .unwrap();
        convert_geographic_bounds_image(
            &source,
            &output,
            TerrainPatch::root(CubeFace::PosZ),
            GeographicBounds {
                west_longitude_deg: 50.0,
                south_latitude_deg: -20.0,
                east_longitude_deg: 130.0,
                north_latitude_deg: 20.0,
            },
            4,
        )
        .unwrap();
        let tile = image::open(output.join("pos_z/0/0_0.png"))
            .unwrap()
            .to_rgba8();
        assert_eq!(tile.get_pixel(2, 2), &Rgba([12, 34, 56, 255]));
        assert_eq!(tile.get_pixel(0, 0), &Rgba([0, 0, 0, 0]));
        fs::remove_file(source).unwrap();
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn equirectangular_sampling_wraps_the_antimeridian() {
        let source = ImageBuffer::from_fn(4, 2, |x, _| Rgba([x as u8, 0, 0, 255]));
        let west = sample_equirectangular(&source, DVec3::new(-1.0, 0.0, 1.0).normalize());
        let east = sample_equirectangular(&source, DVec3::new(-1.0, 0.0, -1.0).normalize());
        assert_ne!(west, east);
    }

    #[test]
    fn equirectangular_sampling_uses_terrain_latitude_longitude_degrees() {
        let source = ImageBuffer::from_fn(360, 180, |x, y| Rgba([x as u8, y as u8, 0, 255]));
        let sample = sample_equirectangular(&source, DVec3::new(0.0, 0.0, 1.0));
        assert_eq!(sample, Rgba([14, 90, 0, 255]));
    }

    #[test]
    fn manifest_rejects_invalid_package_versions() {
        let mut manifest = manifest();
        manifest.package_version += 1;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_the_unconfigured_template() {
        let mut manifest = manifest();
        manifest.source_name = "UNCONFIGURED".into();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn converter_emits_a_complete_root_cover() {
        let source = std::env::temp_dir().join("cosmic_systems_imagery_source.png");
        let output = std::env::temp_dir().join("cosmic_systems_imagery_tiles");
        let image = ImageBuffer::from_fn(8, 4, |x, y| Rgba([x as u8, y as u8, 0, 255]));
        image.save(&source).unwrap();
        convert_equirectangular_image(&source, &output, 0, 0, 2).unwrap();
        for face in CubeFace::ALL {
            assert!(output.join(cube_face_name(face)).join("0/0_0.png").exists());
        }
        fs::remove_file(source).unwrap();
        fs::remove_dir_all(output).unwrap();
    }
}
