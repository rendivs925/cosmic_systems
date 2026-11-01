//! Body-scoped Earth imagery package resolution.
//!
//! This is the smallest presentation contract that answers one question: for a
//! given `TerrainPatch`, which imagery tile should be shown? It resolves the
//! most detailed produced tile at or coarser than the patch level, and falls
//! back to the global overview otherwise. Imagery is presentation-only and
//! never a terrain-height, collision, or physics authority.

use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, TerrainPatch,
};
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

/// Body-fixed geographic window covered by a cube-sphere patch, in degrees.
///
/// A cube-sphere patch is a spherical quadrilateral whose edges are great-circle
/// arcs. Imagery is published from level 8 down, where a patch spans at most
/// 0.35°, so the corner bounding box covers the patch to far better than one
/// texel of either instrument. Longitude is stored as a circular interval
/// (`west_deg > east_deg` means it crosses the antimeridian), and `covers_pole`
/// marks a patch that contains a pole, where every longitude is present.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PatchGeoWindow {
    south_deg: f64,
    north_deg: f64,
    west_deg: f64,
    east_deg: f64,
    covers_pole: bool,
}

impl PatchGeoWindow {
    /// Whether this window overlaps a region's geographic bounds.
    fn overlaps(&self, region: &ImageryLocalRegion) -> bool {
        if self.north_deg < region.south_deg || self.south_deg > region.north_deg {
            return false;
        }
        if self.covers_pole {
            return true;
        }
        // Both intervals are closed. A patch may wrap the antimeridian while a
        // region, being validated as west < east inside [-180, 180], never does,
        // so shifting the patch interval by a full turn settles the wrap.
        [0.0, 360.0, -360.0].iter().any(|shift| {
            let start = self.west_deg + shift;
            let end = self.east_deg + shift;
            start <= region.east_deg && end >= region.west_deg
        })
    }
}

/// The geographic window covering a patch, from its four corner directions.
fn patch_geo_window(patch: &TerrainPatch) -> PatchGeoWindow {
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let corners = [
        face_uv_to_direction(patch.face, u0, v0),
        face_uv_to_direction(patch.face, u1, v0),
        face_uv_to_direction(patch.face, u0, v1),
        face_uv_to_direction(patch.face, u1, v1),
    ];
    let mut south_deg = f64::INFINITY;
    let mut north_deg = f64::NEG_INFINITY;
    let mut covers_pole = false;
    let mut longitudes = [0.0_f64; 4];
    for (index, corner) in corners.iter().enumerate() {
        let (latitude_deg, longitude_deg) = direction_to_lat_lon(*corner);
        south_deg = south_deg.min(latitude_deg);
        north_deg = north_deg.max(latitude_deg);
        covers_pole |= latitude_deg.abs() >= 89.999;
        longitudes[index] = longitude_deg;
    }
    let (west_deg, east_deg) = smallest_longitude_span(longitudes);
    PatchGeoWindow {
        south_deg,
        north_deg,
        west_deg,
        east_deg,
        covers_pole,
    }
}

/// The shortest circular interval containing four longitudes, in degrees.
///
/// The interval is the complement of the largest gap between them, so a patch
/// straddling the antimeridian comes out as a wrapped pair (`west > east`)
/// instead of a 350° span that would overlap every region on the planet.
fn smallest_longitude_span(mut longitudes: [f64; 4]) -> (f64, f64) {
    longitudes.sort_by(|a, b| a.partial_cmp(b).expect("corner longitudes are finite"));
    let mut start_deg = longitudes[0];
    let mut largest_gap_deg = f64::NEG_INFINITY;
    for index in 0..4 {
        let current = longitudes[index];
        let next = if index == 3 {
            longitudes[0] + 360.0
        } else {
            longitudes[index + 1]
        };
        let gap = next - current;
        if gap > largest_gap_deg {
            largest_gap_deg = gap;
            // The interval begins immediately after the largest gap.
            start_deg = next;
        }
    }
    start_deg = wrap_longitude(start_deg);
    let span_deg = 360.0 - largest_gap_deg;
    (start_deg, start_deg + span_deg)
}

/// Wrap a longitude into `[-180, 180)`. `+180` maps to `-180`, the same meridian.
fn wrap_longitude(longitude_deg: f64) -> f64 {
    (longitude_deg + 180.0).rem_euclid(360.0) - 180.0
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

    /// The local region a patch's footprint overlaps in body-fixed coordinates.
    ///
    /// Overlap, not centre containment: a patch that straddles the region edge
    /// must still receive the regional tile, or the cross-fade band could not be
    /// evaluated there and the region edge would fall back on a patch edge.
    pub fn region_for_patch(&self, patch: &TerrainPatch) -> Option<&ImageryLocalRegion> {
        let window = patch_geo_window(patch);
        self.regions.iter().find(|region| window.overlaps(region))
    }

    pub fn tile_path(&self, patch: &TerrainPatch) -> PathBuf {
        self.root
            .join(TILES_DIRECTORY)
            .join(cube_face_name(patch.face))
            .join(patch.level.to_string())
            .join(format!("{}_{}.png", patch.tile_x, patch.tile_y))
    }

    /// Per-channel linear gain that matches a produced tile to the global
    /// overview. Identity outside every region and for regions whose mosaic was
    /// already produced on the overview's tonality.
    ///
    /// Resolution is by overlap for the same reason as [`Self::region_for_patch`]:
    /// a tile that straddles the edge is drawn by patches on both sides, and
    /// matching only its centre would leave the straddling half of the tile
    /// uncorrected — a bright rim exactly where the cross-fade runs. A tile
    /// overlapping two regions with different gains would take the first; the
    /// package currently publishes one region per body, and the manifest
    /// validator does not yet reject overlapping regions.
    pub fn linear_color_gain(&self, tile: &TerrainPatch) -> [f32; 3] {
        self.region_for_patch(tile)
            .map(|region| region.linear_color_gain())
            .unwrap_or([1.0; 3])
    }

    /// Resolve the imagery for a patch: the most detailed produced tile at or
    /// coarser than the patch level inside a covering region, else the global
    /// overview. The search is bounded by the region level range and never
    /// touches the filesystem.
    pub fn resolve(&self, patch: &TerrainPatch) -> ImageryResolution {
        let window = patch_geo_window(patch);
        for region in &self.regions {
            if !window.overlaps(region) || patch.level < region.min_level {
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

    /// A tile inside a region carries that region's measured colour match; a
    /// patch outside every region is left untouched.
    #[test]
    fn color_gain_is_scoped_to_the_region() {
        let root = temp_root("color_gain");
        std::fs::write(root.join(GLOBAL_OVERVIEW_FILE), b"").unwrap();
        let inside = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, 139.5), 12);
        let outside = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(45.0, 10.0), 12);
        write_tile(
            &root,
            cube_face_name(inside.face),
            inside.level,
            inside.tile_x,
            inside.tile_y,
        );
        let package = EarthImageryPackage::load(&root, verified_manifest()).unwrap();
        let expected = EarthImageryManifest::from_ron(MANIFEST)
            .unwrap()
            .local_regions[0]
            .linear_color_gain();
        assert_eq!(package.linear_color_gain(&inside), expected);
        assert_ne!(expected, [1.0; 3], "the fixture must declare a real gain");
        assert_eq!(package.linear_color_gain(&outside), [1.0; 3]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The patch window is a corner bounding box on a sphere, so it has to get
    /// the wrap and the pole right: a wrapped patch read as a 350° span would
    /// claim every region on the planet, and a polar patch has no longitude
    /// edges at all.
    #[test]
    fn patch_windows_handle_longitude_wrap_and_poles() {
        let region = EarthImageryManifest::from_ron(MANIFEST)
            .unwrap()
            .local_regions[0]
            .clone();
        let window = |west, east, south, north| PatchGeoWindow {
            south_deg: south,
            north_deg: north,
            west_deg: west,
            east_deg: east,
            covers_pole: false,
        };
        assert!(window(139.5, 140.5, -9.0, -7.0).overlaps(&region));
        assert!(window(138.5, 139.2, -9.0, -7.0).overlaps(&region));
        assert!(!window(137.0, 138.9, -9.0, -7.0).overlaps(&region));
        assert!(
            !window(179.5, 180.5, -9.0, -7.0).overlaps(&region),
            "a patch across the antimeridian is not near Papua"
        );
        assert!(
            !window(-179.5, -178.5, -9.0, -7.0).overlaps(&region),
            "the wrapped form must not collapse to a planet-wide span"
        );
        let polar = PatchGeoWindow {
            south_deg: 89.0,
            north_deg: 90.0,
            west_deg: 0.0,
            east_deg: 0.0,
            covers_pole: true,
        };
        assert!(!polar.overlaps(&region), "latitude still decides");
        let polar_near = PatchGeoWindow {
            south_deg: -9.0,
            north_deg: -7.0,
            west_deg: 0.0,
            east_deg: 0.0,
            covers_pole: true,
        };
        assert!(
            polar_near.overlaps(&region),
            "a pole patch spans every meridian"
        );
    }

    #[test]
    fn longitude_span_is_the_complement_of_the_largest_gap() {
        assert_eq!(
            smallest_longitude_span([10.0, 13.0, 11.0, 12.0]),
            (10.0, 13.0)
        );
        let (west, east) = smallest_longitude_span([-179.0, -178.0, 179.0, 178.0]);
        assert_eq!((west, east), (178.0, 182.0));
        assert!(west > 180.0 - 3.0 && east <= 180.0 + 3.0);
    }

    /// A patch that straddles the region edge must still resolve to the regional
    /// tile. Under the old centre-containment rule it did not, so the cross-fade
    /// band would have had nowhere to run: the edge would fall back on a patch
    /// boundary exactly where the band is needed.
    #[test]
    fn a_patch_straddling_the_region_edge_still_resolves_to_the_tile() {
        let root = temp_root("resolve_straddle");
        std::fs::write(root.join(GLOBAL_OVERVIEW_FILE), b"").unwrap();
        let manifest = verified_manifest();
        let region = manifest.local_regions[0].clone();

        let patch = (0..256)
            .find_map(|step| {
                let longitude = region.east_deg + 0.0005 + f64::from(step) * 0.0005;
                let patch =
                    TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, longitude), 10);
                let (_, centre_longitude) = direction_to_lat_lon(patch.center_direction());
                (centre_longitude > region.east_deg && patch_geo_window(&patch).overlaps(&region))
                    .then_some(patch)
            })
            .expect("some level-10 patch must straddle the region's east edge");

        write_tile(
            &root,
            cube_face_name(patch.face),
            patch.level,
            patch.tile_x,
            patch.tile_y,
        );
        let package = EarthImageryPackage::load(&root, manifest).unwrap();
        assert!(
            package.region_for_patch(&patch).is_some(),
            "the renderer needs the region to know the feather band"
        );
        assert!(
            matches!(package.resolve(&patch), ImageryResolution::Detailed { .. }),
            "a straddling patch must bind the regional tile"
        );
        let _ = std::fs::remove_dir_all(&root);
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
