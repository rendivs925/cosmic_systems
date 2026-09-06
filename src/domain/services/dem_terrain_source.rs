//! Feature-gated, deterministic cube-sphere DEM terrain authority.
//!
//! The on-disk format stores signed meter elevations above the same mean-radius
//! datum used by [`TerrainSource`]. It is intentionally small and std-only:
//! a fixed header followed by six row-major `i16` faces in [`CubeFace::ALL`]
//! order. Runtime loading is explicit and performs no network or background IO.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::domain::math::DVec3;

use crate::domain::services::cube_sphere::{
    face_uv, face_uv_to_direction, CubeFace, PatchGeometricError, TerrainPatch,
};
use crate::domain::services::terrain_source::{ElevationBounds, SurfaceClass, TerrainSource};

/// Magic bytes for the versioned cube-sphere DEM format.
pub const DEM_MAGIC: [u8; 8] = *b"CSDEM\0\0\0";
/// The only format version currently accepted by this reader.
pub const DEM_FORMAT_VERSION: u32 = 1;
/// ETOPO1's global raw-grid dimensions, including both longitude seam columns.
pub const ETOPO1_COLUMNS: usize = 21_601;
pub const ETOPO1_ROWS: usize = 10_801;

const HEADER_BYTES: usize = 24;
const FACE_COUNT: usize = 6;
const HEIGHT_BYTES: usize = std::mem::size_of::<i16>();
/// Bound startup metadata work and memory while still giving the 2048-sample
/// Earth DEM 256x256 geographic error regions per cube face.
const MAX_METADATA_LEVEL: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DemPatchMetadata {
    min_m: i16,
    max_m: i16,
}

impl DemPatchMetadata {
    fn elevation_bounds_m(self) -> ElevationBounds {
        ElevationBounds::new(f64::from(self.min_m), f64::from(self.max_m))
    }

    fn geometric_error(self) -> PatchGeometricError {
        let bounds = self.elevation_bounds_m();
        PatchGeometricError::from_elevation_bounds(bounds.min_m, bounds.max_m)
    }

    fn combine(self, other: Self) -> Self {
        Self {
            min_m: self.min_m.min(other.min_m),
            max_m: self.max_m.max(other.max_m),
        }
    }
}

/// Read-only aggregate DEM ranges in the existing cube-sphere hierarchy.
/// Levels deeper than `max_level` inherit their nearest indexed ancestor, which
/// remains conservative without inventing detail beyond the source raster.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DemMetadataPyramid {
    levels: Vec<Vec<DemPatchMetadata>>,
}

impl DemMetadataPyramid {
    fn build(resolution: u32, heights_m: &[i16]) -> Self {
        let max_level = metadata_max_level(resolution);
        let mut levels = (0..=max_level).map(|_| Vec::new()).collect::<Vec<_>>();
        let leaf_width = 1usize << max_level;
        let mut leaves = Vec::with_capacity(FACE_COUNT * leaf_width * leaf_width);
        for face in CubeFace::ALL {
            for tile_y in 0..leaf_width {
                for tile_x in 0..leaf_width {
                    leaves.push(sample_patch_bounds(
                        heights_m,
                        resolution as usize,
                        face,
                        max_level,
                        tile_x,
                        tile_y,
                    ));
                }
            }
        }
        levels[max_level as usize] = leaves;

        for level in (0..max_level).rev() {
            let width = 1usize << level;
            let child_width = width * 2;
            let children = &levels[level as usize + 1];
            let mut parents = Vec::with_capacity(FACE_COUNT * width * width);
            for face_index in 0..FACE_COUNT {
                for tile_y in 0..width {
                    for tile_x in 0..width {
                        let child = |x: usize, y: usize| {
                            children[face_index * child_width * child_width + y * child_width + x]
                        };
                        let x = tile_x * 2;
                        let y = tile_y * 2;
                        parents.push(
                            child(x, y)
                                .combine(child(x + 1, y))
                                .combine(child(x, y + 1))
                                .combine(child(x + 1, y + 1)),
                        );
                    }
                }
            }
            levels[level as usize] = parents;
        }

        Self { levels }
    }

    fn metadata_for(&self, patch: &TerrainPatch) -> DemPatchMetadata {
        let level = patch.level.min(self.levels.len() as u32 - 1);
        let shift = patch.level - level;
        let width = 1usize << level;
        let face_index = cube_face_index(patch.face);
        let tile_x = (patch.tile_x >> shift) as usize;
        let tile_y = (patch.tile_y >> shift) as usize;
        self.levels[level as usize][face_index * width * width + tile_y * width + tile_x]
    }

    fn global_bounds(&self) -> ElevationBounds {
        CubeFace::ALL
            .into_iter()
            .map(|face| self.metadata_for(&TerrainPatch::root(face)))
            .reduce(DemPatchMetadata::combine)
            .expect("cube-sphere DEM has six root faces")
            .elevation_bounds_m()
    }
}

/// Failures while loading, validating, or converting a cube-sphere DEM.
#[derive(Debug)]
pub enum DemError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl Display for DemError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "DEM I/O error: {error}"),
            Self::InvalidFormat(message) => write!(formatter, "invalid DEM format: {message}"),
        }
    }
}

impl Error for DemError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidFormat(_) => None,
        }
    }
}

impl From<std::io::Error> for DemError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Decoded cube-sphere DEM data. Each face has `resolution * resolution`
/// row-major signed-meter samples, indexed as `v * resolution + u`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CubeSphereDem {
    resolution: u32,
    heights_m: Vec<i16>,
    metadata: DemMetadataPyramid,
}

impl CubeSphereDem {
    pub fn new(resolution: u32, heights_m: Vec<i16>) -> Result<Self, DemError> {
        let expected_samples = expected_samples(resolution)?;
        if heights_m.len() != expected_samples {
            return Err(DemError::InvalidFormat(format!(
                "expected {expected_samples} height samples for resolution {resolution}, found {}",
                heights_m.len()
            )));
        }
        Ok(Self {
            resolution,
            metadata: DemMetadataPyramid::build(resolution, &heights_m),
            heights_m,
        })
    }

    pub const fn resolution(&self) -> u32 {
        self.resolution
    }

    /// Decode the versioned format from a complete byte buffer.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DemError> {
        if bytes.len() < HEADER_BYTES {
            return Err(DemError::InvalidFormat(
                "file is shorter than the header".into(),
            ));
        }
        if bytes[0..8] != DEM_MAGIC {
            return Err(DemError::InvalidFormat("unexpected magic bytes".into()));
        }
        let version = read_u32(bytes, 8)?;
        if version != DEM_FORMAT_VERSION {
            return Err(DemError::InvalidFormat(format!(
                "unsupported version {version}; expected {DEM_FORMAT_VERSION}"
            )));
        }
        let resolution = read_u32(bytes, 12)?;
        let face_count = read_u32(bytes, 16)?;
        let sample_bytes = read_u32(bytes, 20)?;
        if face_count != FACE_COUNT as u32 {
            return Err(DemError::InvalidFormat(format!(
                "expected {FACE_COUNT} faces in CubeFace order, found {face_count}"
            )));
        }
        if sample_bytes != HEIGHT_BYTES as u32 {
            return Err(DemError::InvalidFormat(format!(
                "expected {HEIGHT_BYTES}-byte signed elevations, found {sample_bytes}"
            )));
        }

        let sample_count = expected_samples(resolution)?;
        let payload_bytes = sample_count.checked_mul(HEIGHT_BYTES).ok_or_else(|| {
            DemError::InvalidFormat("height payload byte count overflows usize".into())
        })?;
        let expected_bytes = HEADER_BYTES
            .checked_add(payload_bytes)
            .ok_or_else(|| DemError::InvalidFormat("DEM byte count overflows usize".into()))?;
        if bytes.len() != expected_bytes {
            return Err(DemError::InvalidFormat(format!(
                "expected {expected_bytes} bytes, found {}",
                bytes.len()
            )));
        }

        let (samples, remainder) = bytes[HEADER_BYTES..].as_chunks::<HEIGHT_BYTES>();
        debug_assert!(remainder.is_empty());
        let heights_m = samples
            .iter()
            .map(|sample| i16::from_le_bytes(*sample))
            .collect();
        Self::new(resolution, heights_m)
    }

    /// Encode the stable versioned format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_BYTES + self.heights_m.len() * HEIGHT_BYTES);
        bytes.extend_from_slice(&DEM_MAGIC);
        bytes.extend_from_slice(&DEM_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.resolution.to_le_bytes());
        bytes.extend_from_slice(&(FACE_COUNT as u32).to_le_bytes());
        bytes.extend_from_slice(&(HEIGHT_BYTES as u32).to_le_bytes());
        for height_m in &self.heights_m {
            bytes.extend_from_slice(&height_m.to_le_bytes());
        }
        bytes
    }

    pub fn read_path(path: impl AsRef<Path>) -> Result<Self, DemError> {
        Self::from_bytes(&fs::read(path)?)
    }

    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<(), DemError> {
        fs::write(path, self.to_bytes())?;
        Ok(())
    }

    fn sample_face_m(&self, face: CubeFace, u: f64, v: f64) -> f64 {
        let resolution = self.resolution as usize;
        let coordinate = |value: f64| value.clamp(0.0, 1.0) * (resolution - 1) as f64;
        let x = coordinate(u);
        let y = coordinate(v);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(resolution - 1);
        let y1 = (y0 + 1).min(resolution - 1);
        let tx = x - x0 as f64;
        let ty = y - y0 as f64;
        let face_offset = cube_face_index(face) * resolution * resolution;
        let height = |column, row| self.heights_m[face_offset + row * resolution + column] as f64;
        let west = height(x0, y0) + (height(x1, y0) - height(x0, y0)) * tx;
        let east = height(x0, y1) + (height(x1, y1) - height(x0, y1)) * tx;
        west + (east - west) * ty
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        self.metadata.metadata_for(patch).geometric_error()
    }

    /// Convert a complete ETOPO1 signed-`i16` little-endian raster into a
    /// cube-sphere DEM. ETOPO1 rows run north to south and its first/last
    /// longitude columns are the same meridian; the converter wraps at the
    /// first column to avoid a duplicated seam.
    pub fn from_etopo1_raw(raw: &[u8], resolution: u32) -> Result<Self, DemError> {
        let expected_bytes = ETOPO1_COLUMNS
            .checked_mul(ETOPO1_ROWS)
            .and_then(|samples| samples.checked_mul(HEIGHT_BYTES))
            .expect("ETOPO1 raw byte count fits usize on supported targets");
        if raw.len() != expected_bytes {
            return Err(DemError::InvalidFormat(format!(
                "ETOPO1 raw input must be {expected_bytes} bytes for {ETOPO1_COLUMNS}x{ETOPO1_ROWS} signed i16 LE samples, found {}",
                raw.len()
            )));
        }

        let samples = expected_samples(resolution)?;
        let mut heights_m = Vec::with_capacity(samples);
        for face in CubeFace::ALL {
            for row in 0..resolution {
                let v = row as f64 / (resolution - 1) as f64;
                for column in 0..resolution {
                    let u = column as f64 / (resolution - 1) as f64;
                    let direction = face_uv_to_direction(face, u, v);
                    let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
                    heights_m
                        .push(sample_etopo1_m(raw, latitude_deg, longitude_deg).round() as i16);
                }
            }
        }
        Self::new(resolution, heights_m)
    }
}

/// A `TerrainSource` backed by an entirely resident, immutable cube-sphere
/// DEM. Sampling never performs file I/O, cache mutation, or fallback lookup.
#[derive(Debug, Clone)]
pub struct DemTerrainSource {
    dem: Arc<CubeSphereDem>,
}

impl DemTerrainSource {
    pub fn from_dem(dem: CubeSphereDem) -> Self {
        Self { dem: Arc::new(dem) }
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, DemError> {
        Ok(Self::from_dem(CubeSphereDem::read_path(path)?))
    }

    pub fn resolution(&self) -> u32 {
        self.dem.resolution()
    }
}

impl TerrainSource for DemTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        if !latitude_deg.is_finite() || !longitude_deg.is_finite() {
            return 0.0;
        }
        let latitude_rad = latitude_deg.clamp(-90.0, 90.0).to_radians();
        let longitude_rad = longitude_deg.to_radians();
        let direction = DVec3::new(
            latitude_rad.cos() * longitude_rad.cos(),
            latitude_rad.sin(),
            latitude_rad.cos() * longitude_rad.sin(),
        );
        let (face, u, v) = face_uv(direction);
        self.dem.sample_face_m(face, u, v)
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        self.dem.metadata.global_bounds()
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        self.dem.patch_geometric_error(patch)
    }

    fn surface_class(&self, latitude_deg: f64, longitude_deg: f64) -> SurfaceClass {
        if self.height_m(latitude_deg, longitude_deg) <= 0.0 {
            SurfaceClass::Ocean
        } else {
            SurfaceClass::Land
        }
    }
}

/// Convert an ETOPO1 raw signed-`i16` little-endian raster into the versioned
/// cube-sphere format. This is an offline tool; it never runs in the simulator.
pub fn convert_etopo1_raw(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    resolution: u32,
) -> Result<(), DemError> {
    let raw = fs::read(input_path)?;
    CubeSphereDem::from_etopo1_raw(&raw, resolution)?.write_path(output_path)
}

fn expected_samples(resolution: u32) -> Result<usize, DemError> {
    if resolution < 2 {
        return Err(DemError::InvalidFormat(
            "cube-sphere DEM resolution must be at least 2".into(),
        ));
    }
    let side = usize::try_from(resolution)
        .map_err(|_| DemError::InvalidFormat("resolution does not fit usize".into()))?;
    side.checked_mul(side)
        .and_then(|samples_per_face| samples_per_face.checked_mul(FACE_COUNT))
        .ok_or_else(|| DemError::InvalidFormat("DEM sample count overflows usize".into()))
}

fn metadata_max_level(resolution: u32) -> u32 {
    (resolution - 1).ilog2().min(MAX_METADATA_LEVEL)
}

fn sample_patch_bounds(
    heights_m: &[i16],
    resolution: usize,
    face: CubeFace,
    level: u32,
    tile_x: usize,
    tile_y: usize,
) -> DemPatchMetadata {
    let span = 1usize << level;
    let max_sample = resolution - 1;
    let first_x = tile_x * max_sample / span;
    let first_y = tile_y * max_sample / span;
    let last_x = ((tile_x + 1) * max_sample).div_ceil(span).min(max_sample);
    let last_y = ((tile_y + 1) * max_sample).div_ceil(span).min(max_sample);
    let face_offset = cube_face_index(face) * resolution * resolution;
    let mut metadata = DemPatchMetadata {
        min_m: i16::MAX,
        max_m: i16::MIN,
    };
    for row in first_y..=last_y {
        for column in first_x..=last_x {
            let height_m = heights_m[face_offset + row * resolution + column];
            metadata.min_m = metadata.min_m.min(height_m);
            metadata.max_m = metadata.max_m.max(height_m);
        }
    }
    metadata
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, DemError> {
    let field = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| DemError::InvalidFormat("truncated header field".into()))?;
    Ok(u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
}

fn cube_face_index(face: CubeFace) -> usize {
    match face {
        CubeFace::PosX => 0,
        CubeFace::NegX => 1,
        CubeFace::PosY => 2,
        CubeFace::NegY => 3,
        CubeFace::PosZ => 4,
        CubeFace::NegZ => 5,
    }
}

fn direction_to_lat_lon(direction: DVec3) -> (f64, f64) {
    (
        direction.y.clamp(-1.0, 1.0).asin().to_degrees(),
        direction.z.atan2(direction.x).to_degrees(),
    )
}

fn sample_etopo1_m(raw: &[u8], latitude_deg: f64, longitude_deg: f64) -> f64 {
    let latitude = latitude_deg.clamp(-90.0, 90.0);
    let longitude = (longitude_deg + 180.0).rem_euclid(360.0) - 180.0;
    let x = (longitude + 180.0) / 360.0 * (ETOPO1_COLUMNS - 1) as f64;
    let y = (90.0 - latitude) / 180.0 * (ETOPO1_ROWS - 1) as f64;
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1) % (ETOPO1_COLUMNS - 1);
    let y1 = (y0 + 1).min(ETOPO1_ROWS - 1);
    let tx = x - x0 as f64;
    let ty = y - y0 as f64;
    let height = |column, row| etopo1_height_m(raw, column, row) as f64;
    let north = height(x0, y0) + (height(x1, y0) - height(x0, y0)) * tx;
    let south = height(x0, y1) + (height(x1, y1) - height(x0, y1)) * tx;
    north + (south - north) * ty
}

fn etopo1_height_m(raw: &[u8], column: usize, row: usize) -> i16 {
    let offset = (row * ETOPO1_COLUMNS + column) * HEIGHT_BYTES;
    i16::from_le_bytes([raw[offset], raw[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_heights(resolution: u32) -> Vec<i16> {
        CubeFace::ALL
            .into_iter()
            .enumerate()
            .flat_map(|(face_index, _)| {
                std::iter::repeat_n(
                    (face_index as i16 + 1) * 100,
                    (resolution * resolution) as usize,
                )
            })
            .collect()
    }

    #[test]
    fn versioned_dem_round_trips_in_cube_face_order() {
        let dem = CubeSphereDem::new(2, face_heights(2)).expect("valid faces");
        let decoded = CubeSphereDem::from_bytes(&dem.to_bytes()).expect("valid binary");
        assert_eq!(decoded, dem);
        for (index, face) in CubeFace::ALL.into_iter().enumerate() {
            assert_eq!(
                decoded.sample_face_m(face, 0.5, 0.5),
                (index as f64 + 1.0) * 100.0
            );
        }
    }

    #[test]
    fn parser_rejects_unknown_versions_and_invalid_payloads() {
        let dem = CubeSphereDem::new(2, face_heights(2)).expect("valid faces");
        let mut unknown_version = dem.to_bytes();
        unknown_version[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert!(matches!(
            CubeSphereDem::from_bytes(&unknown_version),
            Err(DemError::InvalidFormat(_))
        ));
        assert!(matches!(
            CubeSphereDem::from_bytes(&dem.to_bytes()[..HEADER_BYTES]),
            Err(DemError::InvalidFormat(_))
        ));
    }

    #[test]
    fn dem_sampling_is_bilinear_and_matches_cube_face_authority() {
        let mut heights = face_heights(2);
        heights[0..4].copy_from_slice(&[0, 10, 20, 30]);
        let source =
            DemTerrainSource::from_dem(CubeSphereDem::new(2, heights).expect("valid faces"));
        assert_eq!(source.dem.sample_face_m(CubeFace::PosX, 0.5, 0.5), 15.0);
        assert_eq!(source.height_m(0.0, 0.0), 15.0);
    }

    #[test]
    fn metadata_pyramid_tightens_error_for_local_dem_regions() {
        let resolution = 17;
        let mut heights = vec![0; (resolution * resolution * FACE_COUNT as u32) as usize];
        let face_offset =
            cube_face_index(CubeFace::PosX) * resolution as usize * resolution as usize;
        for row in 0..resolution as usize {
            for column in 0..resolution as usize {
                heights[face_offset + row * resolution as usize + column] =
                    if column < 12 { -200 } else { 1_200 };
            }
        }
        let source = DemTerrainSource::from_dem(CubeSphereDem::new(resolution, heights).unwrap());
        let root = TerrainPatch::root(CubeFace::PosX);
        let west = TerrainPatch {
            face: CubeFace::PosX,
            level: 2,
            tile_x: 0,
            tile_y: 0,
        };
        let east = TerrainPatch { tile_x: 3, ..west };

        assert_eq!(
            source.patch_geometric_error(&root).elevation_range_m,
            1_400.0
        );
        assert_eq!(source.patch_geometric_error(&west).elevation_range_m, 0.0);
        assert_eq!(source.patch_geometric_error(&east).elevation_range_m, 0.0);
    }

    #[test]
    fn global_bounds_include_every_cube_face() {
        let source = DemTerrainSource::from_dem(CubeSphereDem::new(2, face_heights(2)).unwrap());

        assert_eq!(
            source.elevation_bounds_m(),
            ElevationBounds::new(100.0, 600.0)
        );
    }

    #[test]
    fn deep_patches_inherit_the_nearest_indexed_metadata() {
        let resolution = 17;
        let mut heights = face_heights(resolution);
        let face_offset =
            cube_face_index(CubeFace::PosX) * resolution as usize * resolution as usize;
        heights[face_offset] = -400;
        heights[face_offset + 1] = 900;
        let source = DemTerrainSource::from_dem(CubeSphereDem::new(resolution, heights).unwrap());
        let indexed = TerrainPatch {
            face: CubeFace::PosX,
            level: 4,
            tile_x: 0,
            tile_y: 0,
        };
        let deep = TerrainPatch {
            level: 12,
            ..indexed
        };

        assert_eq!(
            source.patch_geometric_error(&deep),
            source.patch_geometric_error(&indexed)
        );
    }

    #[test]
    fn etopo1_coordinates_use_north_to_south_rows_and_wrapped_longitude() {
        let north_west = (90.0, -180.0);
        let south_east = (-90.0, 180.0);
        assert_eq!(
            (
                (north_west.1 + 180.0) / 360.0 * (ETOPO1_COLUMNS - 1) as f64,
                0.0
            ),
            (0.0, 0.0)
        );
        assert_eq!(
            ((90.0 - south_east.0) / 180.0 * (ETOPO1_ROWS - 1) as f64),
            (ETOPO1_ROWS - 1) as f64
        );
    }
}
