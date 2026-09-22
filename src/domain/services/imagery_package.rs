//! Body-scoped Earth imagery package resolution.
//!
//! This is the smallest presentation contract that answers one question: for a
//! given `TerrainPatch`, which imagery tile should be shown? It resolves the
//! most detailed produced tile at or coarser than the patch level, and falls
//! back to the global overview otherwise. Imagery is presentation-only and
//! never a terrain-height, collision, or physics authority.

use crate::domain::services::cube_sphere::{direction_to_lat_lon, TerrainPatch};
use crate::domain::services::imagery_tiles::{
    ancestor_at_level, cube_face_from_name, cube_face_name,
};
use crate::domain::value_objects::imagery_manifest::{EarthImageryManifest, ImageryLocalRegion};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// File name of the global overview image inside the package root.
pub const GLOBAL_OVERVIEW_FILE: &str = "global_overview.png";
/// Directory holding the cube-sphere tiles inside the package root.
pub const TILES_DIRECTORY: &str = "tiles";

#[derive(Debug)]
pub enum ImageryPackageError {
    Manifest(String),
    MissingGlobalOverview(PathBuf),
    MissingTiles(PathBuf),
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl std::fmt::Display for ImageryPackageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(message) => write!(formatter, "invalid imagery manifest: {message}"),
            Self::MissingGlobalOverview(path) => {
                write!(
                    formatter,
                    "imagery global overview is missing: {}",
                    path.display()
                )
            }
            Self::MissingTiles(path) => {
                write!(
                    formatter,
                    "imagery tiles directory is missing: {}",
                    path.display()
                )
            }
            Self::Io { path, error } => {
                write!(
                    formatter,
                    "imagery package I/O failed at {}: {error}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ImageryPackageError {}

impl From<std::io::Error> for ImageryPackageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            error,
        }
    }
}

/// The imagery a visible patch should display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageryResolution {
    /// A produced local tile at `patch.level` (at or coarser than the terrain).
    Detailed { patch: TerrainPatch, path: PathBuf },
    /// The global overview fallback.
    Global { path: PathBuf },
}

impl ImageryResolution {
    pub fn path(&self) -> &Path {
        match self {
            Self::Detailed { path, .. } | Self::Global { path } => path,
        }
    }
}

/// A verified, resident Earth imagery package with its produced tiles indexed.
#[derive(Debug)]
pub struct EarthImageryPackage {
    root: PathBuf,
    global_overview: PathBuf,
    regions: Vec<ImageryLocalRegion>,
    available: BTreeSet<TerrainPatch>,
}

impl EarthImageryPackage {
    /// Load and index a package. The manifest must be fully verified; an absent
    /// or invalid package is reported so startup can fall back to the global
    /// albedo without changing terrain or collision data.
    pub fn load(
        root: impl AsRef<Path>,
        manifest: EarthImageryManifest,
    ) -> Result<Self, ImageryPackageError> {
        manifest.validate().map_err(ImageryPackageError::Manifest)?;
        let root = root.as_ref().to_path_buf();
        let global_overview = root.join(GLOBAL_OVERVIEW_FILE);
        if !global_overview.is_file() {
            return Err(ImageryPackageError::MissingGlobalOverview(global_overview));
        }
        let tiles_root = root.join(TILES_DIRECTORY);
        if !tiles_root.is_dir() {
            return Err(ImageryPackageError::MissingTiles(tiles_root));
        }
        let available = scan_tiles(&tiles_root)?;
        Ok(Self {
            root,
            global_overview,
            regions: manifest.local_regions,
            available,
        })
    }

    pub fn global_overview_path(&self) -> &Path {
        &self.global_overview
    }

    pub fn regions(&self) -> &[ImageryLocalRegion] {
        &self.regions
    }

    /// Number of produced tiles indexed at load.
    pub fn tile_count(&self) -> usize {
        self.available.len()
    }

    pub fn tile_path(&self, patch: &TerrainPatch) -> PathBuf {
        self.root
            .join(TILES_DIRECTORY)
            .join(cube_face_name(patch.face))
            .join(patch.level.to_string())
            .join(format!("{}_{}.png", patch.tile_x, patch.tile_y))
    }

    /// Resolve the imagery for a patch: the most detailed produced tile at or
    /// coarser than the patch level inside a covering region, else the global
    /// overview. The search is bounded by the region level range and never
    /// touches the filesystem.
    pub fn resolve(&self, patch: &TerrainPatch) -> ImageryResolution {
        let (latitude, longitude) = direction_to_lat_lon(patch.center_direction());
        for region in &self.regions {
            if !region.contains(latitude, longitude) || patch.level < region.min_level {
                continue;
            }
            let mut level = patch.level.min(region.max_level);
            loop {
                let tile = ancestor_at_level(patch, level);
                if self.available.contains(&tile) {
                    return ImageryResolution::Detailed {
                        patch: tile,
                        path: self.tile_path(&tile),
                    };
                }
                if level == region.min_level {
                    break;
                }
                level -= 1;
            }
        }
        ImageryResolution::Global {
            path: self.global_overview.clone(),
        }
    }
}

fn scan_tiles(tiles_root: &Path) -> Result<BTreeSet<TerrainPatch>, ImageryPackageError> {
    let mut available = BTreeSet::new();
    for face_entry in read_dir(tiles_root)? {
        let face_entry = face_entry?;
        if !face_entry
            .file_type()
            .map(|kind| kind.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(face) = face_entry
            .file_name()
            .to_str()
            .and_then(cube_face_from_name)
        else {
            continue;
        };
        for level_entry in read_dir(&face_entry.path())? {
            let level_entry = level_entry?;
            let Some(level) = level_entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            for tile_entry in read_dir(&level_entry.path())? {
                let tile_entry = tile_entry?;
                let tile_path = tile_entry.path();
                if tile_path.extension().and_then(|e| e.to_str()) != Some("png") {
                    continue;
                }
                let Some(stem) = tile_path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let Some((x, y)) = stem.split_once('_') else {
                    continue;
                };
                let (Ok(tile_x), Ok(tile_y)) = (x.parse::<u32>(), y.parse::<u32>()) else {
                    continue;
                };
                available.insert(TerrainPatch {
                    face,
                    level,
                    tile_x,
                    tile_y,
                });
            }
        }
    }
    Ok(available)
}

fn read_dir(path: &Path) -> Result<std::fs::ReadDir, ImageryPackageError> {
    std::fs::read_dir(path).map_err(|error| ImageryPackageError::Io {
        path: path.to_path_buf(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::reference_frames::terrain_lat_lon_to_body_fixed;

    const MANIFEST: &str = include_str!("../../../assets/configs/terrain/earth_imagery_v1.ron");

    fn verified_manifest() -> EarthImageryManifest {
        let mut manifest = EarthImageryManifest::from_ron(MANIFEST).unwrap();
        let digest = "a".repeat(64);
        manifest.global_overview.source_sha256 = digest.clone();
        manifest.global_overview.runtime_sha256 = digest.clone();
        for region in &mut manifest.local_regions {
            region.source_sha256 = digest.clone();
            region.runtime_sha256 = digest.clone();
        }
        manifest
    }

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("cosmic_imagery_pkg_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_tile(root: &Path, face: &str, level: u32, x: u32, y: u32) {
        let directory = root
            .join(TILES_DIRECTORY)
            .join(face)
            .join(level.to_string());
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{x}_{y}.png")), b"").unwrap();
    }

    #[test]
    fn missing_global_overview_is_rejected() {
        let root = temp_root("missing_global");
        std::fs::create_dir_all(root.join(TILES_DIRECTORY)).unwrap();
        let error = EarthImageryPackage::load(&root, verified_manifest()).unwrap_err();
        assert!(matches!(
            error,
            ImageryPackageError::MissingGlobalOverview(_)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unverified_manifest_is_rejected() {
        let root = temp_root("unverified");
        std::fs::write(root.join(GLOBAL_OVERVIEW_FILE), b"").unwrap();
        std::fs::create_dir_all(root.join(TILES_DIRECTORY)).unwrap();
        let mut manifest = EarthImageryManifest::from_ron(MANIFEST).unwrap();
        manifest.global_overview.source_sha256 = "pending".into();
        let error = EarthImageryPackage::load(&root, manifest).unwrap_err();
        assert!(matches!(error, ImageryPackageError::Manifest(_)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn launch_patch_resolves_to_the_most_detailed_available_tile() {
        let root = temp_root("resolve_detailed");
        std::fs::write(root.join(GLOBAL_OVERVIEW_FILE), b"").unwrap();
        // The launch region declares levels 8..=12. Provide level 10 and a
        // level-12 tile; a level-14 patch must clamp to the finest available.
        let launch = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, 139.5), 14);
        let level12 = ancestor_at_level(&launch, 12);
        let level10 = ancestor_at_level(&launch, 10);
        write_tile(
            &root,
            cube_face_name(level12.face),
            12,
            level12.tile_x,
            level12.tile_y,
        );
        write_tile(
            &root,
            cube_face_name(level10.face),
            10,
            level10.tile_x,
            level10.tile_y,
        );
        let package = EarthImageryPackage::load(&root, verified_manifest()).unwrap();
        assert_eq!(package.tile_count(), 2);

        let ImageryResolution::Detailed { patch, path } = package.resolve(&launch) else {
            panic!("the launch patch must resolve to detailed imagery");
        };
        assert_eq!(patch.level, 12);
        assert_eq!(patch, level12);
        assert_eq!(path, package.tile_path(&level12));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn finer_patch_falls_back_to_a_coarser_available_tile() {
        let root = temp_root("resolve_coarser");
        std::fs::write(root.join(GLOBAL_OVERVIEW_FILE), b"").unwrap();
        let launch = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, 139.5), 14);
        let level8 = ancestor_at_level(&launch, 8);
        write_tile(
            &root,
            cube_face_name(level8.face),
            8,
            level8.tile_x,
            level8.tile_y,
        );
        let package = EarthImageryPackage::load(&root, verified_manifest()).unwrap();

        let ImageryResolution::Detailed { patch, .. } = package.resolve(&launch) else {
            panic!("a coarser produced tile must still be used");
        };
        assert_eq!(patch, level8);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn patch_outside_the_region_uses_the_global_overview() {
        let root = temp_root("resolve_global");
        std::fs::write(root.join(GLOBAL_OVERVIEW_FILE), b"").unwrap();
        write_tile(&root, "neg_x", 8, 0, 0);
        let package = EarthImageryPackage::load(&root, verified_manifest()).unwrap();

        let elsewhere = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(45.0, 10.0), 12);
        let resolution = package.resolve(&elsewhere);
        assert!(matches!(resolution, ImageryResolution::Global { .. }));
        assert_eq!(resolution.path(), package.global_overview_path());
        let _ = std::fs::remove_dir_all(&root);
    }
}
