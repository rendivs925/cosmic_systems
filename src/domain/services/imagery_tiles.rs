//! Offline cube-sphere imagery tile preparation.
//!
//! This module maps an equirectangular RGB source raster onto the existing
//! cube-sphere `TerrainPatch` identity. It is pure domain logic: no Bevy, no
//! runtime downloads, and no terrain-height or collision authority. Sampling
//! uses explicit pixel-center bilinear filtering with longitude wrap
//! (antimeridian) and latitude clamping (poles), so cube-face tiles are
//! continuous across face seams.

use crate::domain::math::DVec3;
use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv, face_uv_to_direction, CubeFace, TerrainPatch,
};
use crate::domain::services::reference_frames::terrain_lat_lon_to_body_fixed;
use crate::domain::value_objects::imagery_manifest::GeoBounds;
use std::collections::BTreeSet;

/// RGBA8 equirectangular source raster. Row 0 is the north pole, column 0 is
/// 180 degrees west, and columns wrap across the antimeridian.
#[derive(Debug, Clone)]
pub struct EquirectangularRgb {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageryTileError {
    InvalidDimensions,
    InvalidPixelCount { expected: usize, actual: usize },
}

impl EquirectangularRgb {
    pub fn new(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Result<Self, ImageryTileError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or(ImageryTileError::InvalidDimensions)?;
        if width < 2 || height < 2 {
            return Err(ImageryTileError::InvalidDimensions);
        }
        if pixels.len() != expected {
            return Err(ImageryTileError::InvalidPixelCount {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }

    /// Bilinear sample at a terrain latitude/longitude in degrees.
    pub fn sample_lat_lon(&self, latitude_deg: f64, longitude_deg: f64) -> [u8; 4] {
        // Pixel centers sit at (i + 0.5) / size, so the sample coordinate is
        // offset by half a pixel. Longitude wraps; latitude clamps at the poles.
        let u = (longitude_deg + 180.0).rem_euclid(360.0) / 360.0;
        let v = ((90.0 - latitude_deg) / 180.0).clamp(0.0, 1.0);
        let x = u * self.width as f64 - 0.5;
        let y = v * self.height as f64 - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let tx = x - x0;
        let ty = y - y0;
        let x0 = x0 as i64;
        let y0 = y0 as i64;

        let texel = |x: i64, y: i64| {
            let wx = x.rem_euclid(self.width as i64) as usize;
            let wy = y.clamp(0, self.height as i64 - 1) as usize;
            self.pixels[wy * self.width as usize + wx]
        };
        let c00 = texel(x0, y0);
        let c10 = texel(x0 + 1, y0);
        let c01 = texel(x0, y0 + 1);
        let c11 = texel(x0 + 1, y0 + 1);
        let mut out = [0u8; 4];
        for channel in 0..4 {
            let top = lerp(c00[channel] as f64, c10[channel] as f64, tx);
            let bottom = lerp(c01[channel] as f64, c11[channel] as f64, tx);
            out[channel] = lerp(top, bottom, ty).round().clamp(0.0, 255.0) as u8;
        }
        out
    }

    /// Sample by body-fixed surface direction.
    pub fn sample_direction(&self, direction: DVec3) -> [u8; 4] {
        let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
        self.sample_lat_lon(latitude_deg, longitude_deg)
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Render one cube-sphere imagery tile at `resolution` texels per side.
///
/// Texel `(i, j)` samples the source through the direction at face-UV
/// `((i + 0.5) / resolution, (j + 0.5) / resolution)` inside the patch, matching
/// the mesh's tile-local UV convention with pixel-center filtering.
pub fn render_imagery_tile(
    source: &EquirectangularRgb,
    patch: &TerrainPatch,
    resolution: u32,
) -> Vec<[u8; 4]> {
    let res = resolution.max(2) as usize;
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let mut pixels = Vec::with_capacity(res * res);
    for j in 0..res {
        for i in 0..res {
            let u = u0 + (u1 - u0) * (i as f64 + 0.5) / res as f64;
            let v = v0 + (v1 - v0) * (j as f64 + 0.5) / res as f64;
            let direction = face_uv_to_direction(patch.face, u, v);
            pixels.push(source.sample_direction(direction));
        }
    }
    pixels
}

/// Stable directory name for a cube face, matching the manifest layout.
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

/// Inverse of [`cube_face_name`] for parsing a package directory.
pub fn cube_face_from_name(name: &str) -> Option<CubeFace> {
    CubeFace::ALL
        .into_iter()
        .find(|face| cube_face_name(*face) == name)
}

/// The patch at `level` that contains a finer patch, by shifting tile indices.
pub fn ancestor_at_level(patch: &TerrainPatch, level: u32) -> TerrainPatch {
    let shift = patch.level.saturating_sub(level);
    TerrainPatch {
        face: patch.face,
        level,
        tile_x: patch.tile_x >> shift,
        tile_y: patch.tile_y >> shift,
    }
}

/// Number of samples per axis used to bound a region's face coverage.
const REGION_SAMPLES: usize = 64;

/// Every cube-sphere tile at `level` that overlaps a local region.
///
/// The region is bounded by sampling a regular latitude/longitude grid, so a
/// region spanning multiple cube faces or a cube edge is handled without
/// special cases.
pub fn tiles_for_region(region: GeoBounds, level: u32) -> BTreeSet<TerrainPatch> {
    let mut tiles = BTreeSet::new();
    for face in CubeFace::ALL {
        let Some((tx0, ty0, tx1, ty1)) = region_face_tile_bounds(region, face, level) else {
            continue;
        };
        for tile_y in ty0..=ty1 {
            for tile_x in tx0..=tx1 {
                let patch = TerrainPatch {
                    face,
                    level,
                    tile_x,
                    tile_y,
                };
                if patch_overlaps_region(region, &patch) {
                    tiles.insert(patch);
                }
            }
        }
    }
    tiles
}

fn region_face_tile_bounds(
    region: GeoBounds,
    face: CubeFace,
    level: u32,
) -> Option<(u32, u32, u32, u32)> {
    let mut min_u = f64::INFINITY;
    let mut min_v = f64::INFINITY;
    let mut max_u = f64::NEG_INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    let mut found = false;
    for i in 0..=REGION_SAMPLES {
        for j in 0..=REGION_SAMPLES {
            let latitude = lerp(
                region.south_deg,
                region.north_deg,
                j as f64 / REGION_SAMPLES as f64,
            );
            let longitude = lerp(
                region.west_deg,
                region.east_deg,
                i as f64 / REGION_SAMPLES as f64,
            );
            let direction = terrain_lat_lon_to_body_fixed(latitude, longitude);
            let (sample_face, u, v) = face_uv(direction);
            if sample_face != face {
                continue;
            }
            found = true;
            min_u = min_u.min(u);
            min_v = min_v.min(v);
            max_u = max_u.max(u);
            max_v = max_v.max(v);
        }
    }
    if !found {
        return None;
    }
    let span = (1u64 << level) as f64;
    let last = (1u64 << level) - 1;
    let tx0 = (min_u * span).floor().clamp(0.0, last as f64) as u32;
    let ty0 = (min_v * span).floor().clamp(0.0, last as f64) as u32;
    let tx1 = (max_u * span).floor().clamp(0.0, last as f64) as u32;
    let ty1 = (max_v * span).floor().clamp(0.0, last as f64) as u32;
    Some((tx0, ty0, tx1, ty1))
}

fn patch_overlaps_region(region: GeoBounds, patch: &TerrainPatch) -> bool {
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let samples = [
        (u0, v0),
        (u1, v0),
        (u0, v1),
        (u1, v1),
        ((u0 + u1) * 0.5, (v0 + v1) * 0.5),
        ((u0 + u1) * 0.5, v0),
        ((u0 + u1) * 0.5, v1),
        (u0, (v0 + v1) * 0.5),
        (u1, (v0 + v1) * 0.5),
    ];
    samples.into_iter().any(|(u, v)| {
        let direction = face_uv_to_direction(patch.face, u, v);
        let (latitude, longitude) = direction_to_lat_lon(direction);
        region.contains(latitude, longitude)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source whose colour encodes its geographic coordinate, so tests can
    /// detect any mapping error.
    fn coordinate_source(width: u32, height: u32) -> EquirectangularRgb {
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.push([
                    ((x as f64 + 0.5) / width as f64 * 255.0) as u8,
                    ((y as f64 + 0.5) / height as f64 * 255.0) as u8,
                    0,
                    255,
                ]);
            }
        }
        EquirectangularRgb::new(width, height, pixels).unwrap()
    }

    fn region() -> GeoBounds {
        GeoBounds {
            west_deg: 139.0,
            south_deg: -8.6,
            east_deg: 140.0,
            north_deg: -7.4,
        }
    }

    #[test]
    fn sample_rejects_mismatched_pixel_counts() {
        assert!(matches!(
            EquirectangularRgb::new(4, 4, vec![[0, 0, 0, 255]; 15]),
            Err(ImageryTileError::InvalidPixelCount { .. })
        ));
        assert!(matches!(
            EquirectangularRgb::new(1, 4, vec![[0, 0, 0, 255]; 4]),
            Err(ImageryTileError::InvalidDimensions)
        ));
    }

    #[test]
    fn sampling_is_pixel_centered() {
        // A 2x2 image where each texel has a distinct colour.
        let source = EquirectangularRgb::new(
            2,
            2,
            vec![
                [10, 0, 0, 255],
                [20, 0, 0, 255],
                [30, 0, 0, 255],
                [40, 0, 0, 255],
            ],
        )
        .unwrap();
        // Centers of the four texels: lon -90/+90, lat +45/-45.
        assert_eq!(source.sample_lat_lon(45.0, -90.0), [10, 0, 0, 255]);
        assert_eq!(source.sample_lat_lon(45.0, 90.0), [20, 0, 0, 255]);
        assert_eq!(source.sample_lat_lon(-45.0, -90.0), [30, 0, 0, 255]);
        assert_eq!(source.sample_lat_lon(-45.0, 90.0), [40, 0, 0, 255]);
    }

    #[test]
    fn longitude_wraps_across_the_antimeridian() {
        let source = EquirectangularRgb::new(
            4,
            2,
            vec![
                [255, 0, 0, 255],
                [0, 0, 0, 255],
                [0, 0, 0, 255],
                [0, 255, 0, 255],
                [255, 0, 0, 255],
                [0, 0, 0, 255],
                [0, 0, 0, 255],
                [0, 255, 0, 255],
            ],
        )
        .unwrap();
        // Exactly on the seam, the blend uses the last and first columns.
        let seam = source.sample_lat_lon(0.0, 180.0);
        assert!(
            seam[0] > 0 && seam[1] > 0,
            "seam must blend both edges: {seam:?}"
        );
        // Longitudes beyond the seam wrap rather than clamp.
        assert_eq!(
            source.sample_lat_lon(0.0, 180.0),
            source.sample_lat_lon(0.0, -180.0)
        );
    }

    #[test]
    fn latitude_clamps_at_the_poles() {
        let source = coordinate_source(8, 8);
        assert_eq!(
            source.sample_lat_lon(90.0, 0.0),
            source.sample_lat_lon(120.0, 0.0)
        );
        assert_eq!(
            source.sample_lat_lon(-90.0, 0.0),
            source.sample_lat_lon(-120.0, 0.0)
        );
    }

    #[test]
    fn tile_texel_centers_map_to_patch_uv_centers() {
        let source = coordinate_source(512, 256);
        let patch = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, 139.5), 6);
        let resolution = 4;
        let tile = render_imagery_tile(&source, &patch, resolution);
        let (u0, v0, u1, v1) = patch.uv_bounds();
        for j in 0..resolution as usize {
            for i in 0..resolution as usize {
                let u = u0 + (u1 - u0) * (i as f64 + 0.5) / resolution as f64;
                let v = v0 + (v1 - v0) * (j as f64 + 0.5) / resolution as f64;
                let expected = source.sample_direction(face_uv_to_direction(patch.face, u, v));
                assert_eq!(tile[j * resolution as usize + i], expected);
            }
        }
    }

    #[test]
    fn adjacent_tile_edges_are_continuous() {
        let source = coordinate_source(1024, 512);
        let patch = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, 139.5), 5);
        let resolution = 8;
        let left = render_imagery_tile(&source, &patch, resolution);
        let east = TerrainPatch {
            tile_x: patch.tile_x + 1,
            ..patch
        };
        let right = render_imagery_tile(&source, &east, resolution);
        for j in 0..resolution as usize {
            let left_edge = left[j * resolution as usize + resolution as usize - 1];
            let right_edge = right[j * resolution as usize];
            // The edge texels are one source texel apart at most.
            for channel in 0..3 {
                let difference = (left_edge[channel] as i32 - right_edge[channel] as i32).abs();
                assert!(
                    difference <= 4,
                    "cube-edge seam discontinuity: {difference}"
                );
            }
        }
    }

    #[test]
    fn region_tiles_cover_the_launch_site_and_stay_bounded() {
        let region = region();
        for level in [8u32, 10, 12] {
            let tiles = tiles_for_region(region, level);
            assert!(!tiles.is_empty(), "no tiles at level {level}");
            let launch =
                TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, 139.5), level);
            assert!(
                tiles.contains(&launch),
                "launch tile missing at level {level}"
            );
            // A one-degree region must not select a large fraction of the face.
            assert!(
                tiles.len() < (1usize << (level * 2)) / 8,
                "region selection is not bounded at level {level}: {}",
                tiles.len()
            );
        }
    }

    #[test]
    fn tile_rendering_is_deterministic() {
        let source = coordinate_source(256, 128);
        let patch = TerrainPatch::for_direction(terrain_lat_lon_to_body_fixed(-8.0, 139.5), 7);
        assert_eq!(
            render_imagery_tile(&source, &patch, 8),
            render_imagery_tile(&source, &patch, 8)
        );
    }
}
