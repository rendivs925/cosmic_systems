//! Offline converter from a resident cube-sphere DEM (`*.csdem`) into a
//! versioned cube-sphere elevation tile-payload pyramid (`*.cstile` tiles plus
//! a `manifest.ron` package index).
//!
//! Requires `--features dem`. It samples the resident DEM through the same
//! authoritative bilinear face-UV accessor the runtime uses, writing one tile
//! per cube-sphere patch from level 0 through `max_level`. Generation is
//! streamed tile-by-tile and deterministic: identical input bytes and arguments
//! produce byte-identical tiles and manifest.

use std::env;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cosmic_systems_wasm::domain::services::cube_sphere::{CubeFace, TerrainPatch};
use cosmic_systems_wasm::domain::services::dem_terrain_source::{CubeSphereDem, DemError};
use cosmic_systems_wasm::domain::services::elevation_pyramid::{
    ElevationPyramidError, ElevationPyramidManifest, ElevationTile, ElevationTileMetadata,
    ELEVATION_PYRAMID_FORMAT_VERSION,
};
use sha2::{Digest, Sha256};

/// Default deepest tile level. Level `n` splits each of the six cube faces into
/// `2^n * 2^n` tiles, so the default builds a complete-planet pyramid.
const DEFAULT_MAX_LEVEL: u32 = 8;
/// Default tile sample count per side (`64 * 64` grid with shared edges).
const DEFAULT_TILE_RESOLUTION: u32 = 64;
/// Matches the payload manifest's defensive level ceiling (`MAX_PAYLOAD_LEVEL`).
const MAX_LEVEL_LIMIT: u32 = 20;
/// Upper bound on tile samples per side so one tile's working set stays bounded.
const MAX_TILE_RESOLUTION: u32 = 4_096;
/// Earth mean radius, used only to document the source ground sample distance.
const EARTH_MEAN_RADIUS_M: f64 = 6_371_000.0;

// Documented provenance defaults. The CLI deliberately keeps the positional
// argument surface small; edit these constants (or the emitted manifest) when
// publishing a package under a specific source and license.
const DEFAULT_BODY: &str = "Earth";
const DEFAULT_COORDINATE_FRAME: &str = "cube-sphere-face-uv";
const DEFAULT_VERTICAL_DATUM: &str = "MSL";
const DEFAULT_SOURCE: &str = "resident CSDEM resampled to elevation tile payloads";
const DEFAULT_LICENSE: &str = "see source dataset license";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("elevation pyramid conversion failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Everything needed to build a manifest, filled from arguments and documented
/// defaults before any output is written.
struct ConversionOptions {
    max_level: u32,
    tile_resolution: u32,
    body: String,
    coordinate_frame: String,
    vertical_datum: String,
    source: String,
    license: String,
    source_sha256: String,
    resolution_m: f64,
}

/// Normal-runtime conversion failures; the process reports these rather than
/// panicking, matching the other offline converter binaries.
#[derive(Debug)]
enum ConvertError {
    Usage(String),
    Io(std::io::Error),
    Dem(DemError),
    Pyramid(ElevationPyramidError),
}

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(formatter, "{message}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Dem(error) => write!(formatter, "{error}"),
            Self::Pyramid(ElevationPyramidError::Io(error)) => {
                write!(formatter, "I/O error: {error}")
            }
            Self::Pyramid(ElevationPyramidError::InvalidFormat(message)) => {
                write!(formatter, "invalid elevation payload: {message}")
            }
        }
    }
}

impl From<std::io::Error> for ConvertError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<DemError> for ConvertError {
    fn from(error: DemError) -> Self {
        Self::Dem(error)
    }
}

impl From<ElevationPyramidError> for ConvertError {
    fn from(error: ElevationPyramidError) -> Self {
        Self::Pyramid(error)
    }
}

fn run() -> Result<(), ConvertError> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let (input_path, output_dir, max_level, tile_resolution) = parse_arguments(&arguments)?;

    // Read once so the same bytes supply both the decoded authority and its
    // provenance checksum.
    let source_bytes = fs::read(&input_path)?;
    let source_sha256 = hex_sha256(&source_bytes);
    let dem = CubeSphereDem::from_bytes(&source_bytes)?;

    let options = ConversionOptions {
        max_level,
        tile_resolution,
        body: DEFAULT_BODY.to_string(),
        coordinate_frame: DEFAULT_COORDINATE_FRAME.to_string(),
        vertical_datum: DEFAULT_VERTICAL_DATUM.to_string(),
        source: DEFAULT_SOURCE.to_string(),
        license: DEFAULT_LICENSE.to_string(),
        source_sha256,
        resolution_m: source_resolution_m(dem.resolution()),
    };

    fs::create_dir_all(&output_dir)?;
    let manifest = convert_dem(&dem, &options, &output_dir)?;
    fs::write(output_dir.join("manifest.ron"), manifest.to_ron_string()?)?;
    eprintln!(
        "wrote {} tiles (level <= {max_level}, {tile_resolution}x{tile_resolution}) to {}",
        manifest.tiles.len(),
        output_dir.display()
    );
    Ok(())
}

fn parse_arguments(arguments: &[String]) -> Result<(PathBuf, PathBuf, u32, u32), ConvertError> {
    if !(2..=4).contains(&arguments.len()) {
        return Err(ConvertError::Usage(usage_text()));
    }
    let input_path = PathBuf::from(&arguments[0]);
    let output_dir = PathBuf::from(&arguments[1]);

    let max_level = parse_optional(arguments.get(2), DEFAULT_MAX_LEVEL, "max_level")?;
    if max_level > MAX_LEVEL_LIMIT {
        return Err(ConvertError::Usage(format!(
            "max_level must be <= {MAX_LEVEL_LIMIT}, found {max_level}"
        )));
    }
    let tile_resolution =
        parse_optional(arguments.get(3), DEFAULT_TILE_RESOLUTION, "tile_resolution")?;
    if !(2..=MAX_TILE_RESOLUTION).contains(&tile_resolution) {
        return Err(ConvertError::Usage(format!(
            "tile_resolution must be between 2 and {MAX_TILE_RESOLUTION}, found {tile_resolution}"
        )));
    }
    Ok((input_path, output_dir, max_level, tile_resolution))
}

fn parse_optional(value: Option<&String>, default: u32, name: &str) -> Result<u32, ConvertError> {
    match value {
        Some(text) => text.parse::<u32>().map_err(|_| {
            ConvertError::Usage(format!(
                "{name} must be a non-negative integer, found `{text}`"
            ))
        }),
        None => Ok(default),
    }
}

fn usage_text() -> String {
    format!(
        "usage: elevation_pyramid_convert <input.csdem> <output_dir> [max_level] [tile_resolution]\n\
         defaults: max_level={DEFAULT_MAX_LEVEL}, tile_resolution={DEFAULT_TILE_RESOLUTION}"
    )
}

/// Build the pyramid and write each tile, returning the validated manifest.
///
/// Tiles are written and dropped one at a time; only the (small) metadata index
/// is retained, so peak memory is one tile plus the manifest. Iteration order is
/// fixed — level, then [`CubeFace::ALL`], then `tile_y`, then `tile_x` — so the
/// manifest and on-disk layout are reproducible.
fn convert_dem(
    dem: &CubeSphereDem,
    options: &ConversionOptions,
    output_dir: &Path,
) -> Result<ElevationPyramidManifest, ConvertError> {
    let mut tiles: Vec<ElevationTileMetadata> = Vec::new();
    for level in 0..=options.max_level {
        let span = 1u64 << level;
        for face in CubeFace::ALL {
            let face_dir = output_dir
                .join("tiles")
                .join(face_directory(face))
                .join(level.to_string());
            fs::create_dir_all(&face_dir)?;
            for tile_y in 0..span {
                for tile_x in 0..span {
                    let patch = TerrainPatch {
                        face,
                        level,
                        tile_x: tile_x as u32,
                        tile_y: tile_y as u32,
                    };
                    let tile = sample_tile(dem, patch, options.tile_resolution)?;
                    let payload_path = tile_relative_path(patch);
                    let bytes = tile.to_bytes();
                    fs::write(output_dir.join(&payload_path), &bytes)?;
                    let mut metadata = tile.metadata(payload_path);
                    // Record the checksum of the complete artifact so a verifier
                    // can validate the file end-to-end.
                    metadata.payload_sha256 = hex_sha256(&bytes);
                    tiles.push(metadata);
                }
            }
        }
    }

    let manifest = ElevationPyramidManifest {
        format_version: ELEVATION_PYRAMID_FORMAT_VERSION,
        body: options.body.clone(),
        coordinate_frame: options.coordinate_frame.clone(),
        vertical_datum: options.vertical_datum.clone(),
        source: options.source.clone(),
        license: options.license.clone(),
        source_sha256: options.source_sha256.clone(),
        resolution_m: options.resolution_m,
        tiles,
    };
    manifest.validate()?;
    Ok(manifest)
}

/// Sample one patch as a `resolution * resolution` face-UV grid with shared
/// edges (both endpoints of each axis are included), so adjacent tiles agree on
/// their seam samples.
fn sample_tile(
    dem: &CubeSphereDem,
    patch: TerrainPatch,
    resolution: u32,
) -> Result<ElevationTile, ConvertError> {
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let last = f64::from(resolution - 1);
    let mut samples_m = Vec::with_capacity((resolution as usize) * (resolution as usize));
    let mut min_elevation_m = f64::INFINITY;
    let mut max_elevation_m = f64::NEG_INFINITY;
    for row in 0..resolution {
        let v = v0 + (v1 - v0) * (f64::from(row) / last);
        for column in 0..resolution {
            let u = u0 + (u1 - u0) * (f64::from(column) / last);
            // Bounds are tracked over the stored f32 samples so the declared
            // interval always contains the decoded payload exactly.
            let sample_m = dem.sample_face_m(patch.face, u, v) as f32;
            samples_m.push(sample_m);
            min_elevation_m = min_elevation_m.min(f64::from(sample_m));
            max_elevation_m = max_elevation_m.max(f64::from(sample_m));
        }
    }
    // Conservative geometric error: the tile's full elevation range bounds how
    // far its linear interpolant can deviate from the source inside the patch.
    // This over-estimates smooth terrain deliberately, matching the existing
    // `PatchGeometricError::from_elevation_bounds` convention.
    let geometric_error_m = (max_elevation_m - min_elevation_m).max(0.0);
    ElevationTile::from_samples(
        patch,
        resolution,
        resolution,
        min_elevation_m,
        max_elevation_m,
        geometric_error_m,
        samples_m,
    )
    .map_err(ConvertError::from)
}

/// Tile payload path relative to the package root, e.g.
/// `tiles/pos_z/3/2_5.cstile`.
fn tile_relative_path(patch: TerrainPatch) -> String {
    format!(
        "tiles/{}/{}/{}_{}.cstile",
        face_directory(patch.face),
        patch.level,
        patch.tile_x,
        patch.tile_y
    )
}

fn face_directory(face: CubeFace) -> &'static str {
    match face {
        CubeFace::PosX => "pos_x",
        CubeFace::NegX => "neg_x",
        CubeFace::PosY => "pos_y",
        CubeFace::NegY => "neg_y",
        CubeFace::PosZ => "pos_z",
        CubeFace::NegZ => "neg_z",
    }
}

/// Approximate source ground sample distance: the cube-face arc length divided
/// by the face sample count. The true per-sample spacing varies slightly across
/// a curved cube face, so this is one scalar provenance value, not a per-pixel
/// measurement.
fn source_resolution_m(face_resolution: u32) -> f64 {
    let face_arc_m = EARTH_MEAN_RADIUS_M * std::f64::consts::FRAC_PI_2;
    face_arc_m / f64::from(face_resolution.saturating_sub(1).max(1))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_dem(resolution: u32) -> CubeSphereDem {
        let mut heights_m = Vec::with_capacity((resolution * resolution * 6) as usize);
        for face_index in 0..6i16 {
            for row in 0..resolution {
                for column in 0..resolution {
                    heights_m.push(face_index * 100 + row as i16 * 10 + column as i16);
                }
            }
        }
        CubeSphereDem::new(resolution, heights_m).expect("synthetic DEM")
    }

    fn small_options() -> ConversionOptions {
        ConversionOptions {
            max_level: 1,
            tile_resolution: 3,
            body: DEFAULT_BODY.to_string(),
            coordinate_frame: DEFAULT_COORDINATE_FRAME.to_string(),
            vertical_datum: DEFAULT_VERTICAL_DATUM.to_string(),
            source: "test".to_string(),
            license: "test".to_string(),
            source_sha256: "a".repeat(64),
            resolution_m: 1_000.0,
        }
    }

    /// A pid-scoped scratch directory; each test clears its own path first so
    /// repeated local runs start clean without a tempfile dependency.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "elevation_pyramid_convert_{}_{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch directory");
        dir
    }

    #[test]
    fn tile_slicing_is_deterministic_and_bounds_its_samples() {
        let dem = synthetic_dem(5);
        let patch = TerrainPatch {
            face: CubeFace::PosZ,
            level: 1,
            tile_x: 1,
            tile_y: 0,
        };
        let first = sample_tile(&dem, patch, 4).expect("first tile");
        let second = sample_tile(&dem, patch, 4).expect("second tile");

        assert_eq!(first.patch(), patch);
        assert_eq!(first.to_bytes(), second.to_bytes());
        let (min_m, max_m) = first.elevation_bounds_m();
        assert!(min_m <= max_m);
        assert_eq!(first.geometric_error_m(), max_m - min_m);

        // The corner sample must reproduce the DEM's own face-UV authority.
        let (u0, v0, _, _) = patch.uv_bounds();
        assert_eq!(
            first.sample_uv(u0, v0),
            Some(f64::from(dem.sample_face_m(patch.face, u0, v0) as f32))
        );
    }

    #[test]
    fn manifest_ron_round_trips() {
        let dem = synthetic_dem(4);
        let dir = scratch_dir("ron");
        let manifest = convert_dem(&dem, &small_options(), &dir).expect("convert");

        let text = manifest.to_ron_string().expect("encode manifest");
        let decoded = ElevationPyramidManifest::from_ron_str(&text).expect("decode manifest");
        assert_eq!(decoded, manifest);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn repeat_conversion_is_byte_identical() {
        let dem = synthetic_dem(4);
        let first_dir = scratch_dir("repeat_a");
        let second_dir = scratch_dir("repeat_b");
        let options = small_options();

        let first = convert_dem(&dem, &options, &first_dir).expect("first conversion");
        let second = convert_dem(&dem, &options, &second_dir).expect("second conversion");

        assert_eq!(first, second);
        assert_eq!(
            first.to_ron_string().expect("first manifest"),
            second.to_ron_string().expect("second manifest")
        );
        for tile in &first.tiles {
            assert_eq!(
                fs::read(first_dir.join(&tile.payload_path)).expect("first tile"),
                fs::read(second_dir.join(&tile.payload_path)).expect("second tile"),
                "tile {} must be byte-identical",
                tile.payload_path
            );
            assert_eq!(
                tile.payload_sha256,
                hex_sha256(&fs::read(first_dir.join(&tile.payload_path)).expect("tile"))
            );
        }

        let _ = fs::remove_dir_all(&first_dir);
        let _ = fs::remove_dir_all(&second_dir);
    }
}
