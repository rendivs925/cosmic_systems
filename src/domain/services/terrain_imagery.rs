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

    pub fn tile_path(&self, patch: TerrainPatch) -> Option<String> {
        (patch.level >= self.min_level && patch.level <= self.max_level).then(|| {
            format!(
                "{}/{}/{}/{}_{}.png",
                self.tile_root.trim_end_matches('/'),
                cube_face_name(patch.face),
                patch.level,
                patch.tile_x,
                patch.tile_y
            )
        })
    }
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

fn sample_equirectangular(source: &RgbaImage, direction: DVec3) -> Rgba<u8> {
    let (latitude_rad, longitude_rad) = direction_to_lat_lon(direction);
    let width = source.width();
    let height = source.height();
    let u = (longitude_rad / std::f64::consts::TAU + 0.5).rem_euclid(1.0);
    let v = (0.5 - latitude_rad / std::f64::consts::PI).clamp(0.0, 1.0);
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
        }
    }

    #[test]
    fn manifest_uses_stable_cube_patch_paths() {
        let path = manifest()
            .tile_path(TerrainPatch::root(CubeFace::PosZ))
            .unwrap();
        assert_eq!(
            path,
            "large_files/terrain/earth_imagery_v1/tiles/pos_z/0/0_0.png"
        );
    }

    #[test]
    fn equirectangular_sampling_wraps_the_antimeridian() {
        let source = ImageBuffer::from_fn(4, 2, |x, _| Rgba([x as u8, 0, 0, 255]));
        let west = sample_equirectangular(&source, DVec3::new(-1.0, 0.0, 1.0).normalize());
        let east = sample_equirectangular(&source, DVec3::new(-1.0, 0.0, -1.0).normalize());
        assert_ne!(west, east);
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
