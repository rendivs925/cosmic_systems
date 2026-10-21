//! Offline regional DEM baker. Requires `--features dem`.
//!
//! Usage:
//!   regional_dem_bake [--region ID] [--config PATH] [--root PATH]
//!                     [--output PATH]
//!
//! The baker writes sparse cube-sphere elevation payload tiles under
//! `<output>/regions/<id>/tiles/` and a package index at `<output>/manifest.ron`.
//! It samples reviewed regional source rasters, adds the runtime EGM2008 geoid,
//! and reuses the same tile encoder as `elevation_pyramid_convert`.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use cosmic_systems_wasm::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, TerrainPatch,
};
use cosmic_systems_wasm::domain::services::elevation_pyramid::{
    sample_elevation_tile, ElevationPyramidManifest, ElevationTile, TileElevationSource,
    ELEVATION_PYRAMID_FORMAT_VERSION,
};
use cosmic_systems_wasm::domain::services::geoid::{GeoidError, GeoidModel};
use cosmic_systems_wasm::domain::services::global_raster::GlobalRasterSpec;
use cosmic_systems_wasm::domain::services::reference_frames::terrain_lat_lon_to_body_fixed;
use cosmic_systems_wasm::domain::services::regional_dem::{
    RegionDefinition, RegionalDem, RegionalDemConfig, RegionalDemError,
};
use cosmic_systems_wasm::domain::services::terrain_source::{ElevationBounds, TerrainSource};
use sha2::{Digest, Sha256};
use std::sync::Arc;

const DEFAULT_REGION: &str = "papua_coastal_lowland";
const DEFAULT_CONFIG: &str = "assets/configs/terrain/regions_v1.ron";
const DEFAULT_ROOT: &str = ".";
const DEFAULT_OUTPUT: &str = "assets/large_files/terrain/earth_elevation_pyramid";
/// Patch selection overshoots the reviewed bounds so edge tiles are included.
const PATCH_MARGIN_DEG: f64 = 0.02;
/// Direction-grid step used to discover every patch intersecting the region.
const PATCH_DISCOVERY_STEP_DEG: f64 = 0.004;

#[derive(Debug)]
enum BakeError {
    Arguments(String),
    Config(RegionalDemError),
    Geoid(GeoidError),
    Pyramid(cosmic_systems_wasm::domain::services::elevation_pyramid::ElevationPyramidError),
    Io(std::io::Error),
}

impl std::fmt::Display for BakeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arguments(message) => write!(formatter, "{message}"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Geoid(error) => write!(formatter, "{error}"),
            Self::Pyramid(error) => write!(formatter, "{error:?}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
        }
    }
}

impl From<std::io::Error> for BakeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<RegionalDemError> for BakeError {
    fn from(error: RegionalDemError) -> Self {
        Self::Config(error)
    }
}

impl From<GeoidError> for BakeError {
    fn from(error: GeoidError) -> Self {
        Self::Geoid(error)
    }
}

impl From<cosmic_systems_wasm::domain::services::elevation_pyramid::ElevationPyramidError>
    for BakeError
{
    fn from(
        error: cosmic_systems_wasm::domain::services::elevation_pyramid::ElevationPyramidError,
    ) -> Self {
        Self::Pyramid(error)
    }
}

struct Options {
    region: String,
    config: PathBuf,
    root: PathBuf,
    output: PathBuf,
    probe: Option<(f64, f64)>,
    verify: bool,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return usage();
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("regional DEM bake failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: Options) -> Result<(), BakeError> {
    let config = RegionalDemConfig::from_path(&options.config)?;
    let region = config
        .region(&options.region)
        .cloned()
        .ok_or_else(|| BakeError::Arguments(format!("unknown region {}", options.region)))?;
    let geoid = GeoidModel::from_path(
        options.root.join(&region.geoid_grid),
        GlobalRasterSpec::egm2008_geoid_0p5deg(),
    )?;
    let dem = RegionalDem::load(region.clone(), &options.root, geoid)?;

    let patches = patches_for_region(&region);
    fs::create_dir_all(&options.output)?;
    let mut tiles = Vec::with_capacity(patches.len());
    for patch in patches {
        let tile = sample_elevation_tile(patch, region.tile_resolution, |face, u, v| {
            let direction = face_uv_to_direction(face, u, v);
            let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
            dem.height_ellipsoid_m(latitude_deg, longitude_deg)
                .unwrap_or(0.0)
        })?;
        let column = patch.tile_x;
        let row = patch.tile_y;
        let payload_path = format!(
            "regions/{}/tiles/{}/{}/{}_{}.cstile",
            region.id,
            face_name(patch.face),
            patch.level,
            column,
            row
        );
        let path = options.output.join(&payload_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = tile.to_bytes();
        fs::write(&path, &bytes)?;
        let mut metadata = tile.metadata(payload_path);
        metadata.payload_sha256 = format!("{:x}", Sha256::digest(&bytes));
        tiles.push(metadata);
    }

    let manifest = ElevationPyramidManifest {
        format_version: ELEVATION_PYRAMID_FORMAT_VERSION,
        body: region.body.clone(),
        coordinate_frame: "cube-sphere-face-uv".into(),
        vertical_datum: region.vertical_datum.clone(),
        source: format!("{} (geoid-corrected)", region.source_name),
        license: region.license.clone(),
        source_sha256: dem.source_sha256().to_string(),
        resolution_m: region.resolution_m,
        tiles,
    };
    manifest.validate()?;
    fs::write(
        options.output.join("manifest.ron"),
        manifest.to_ron_string()?,
    )?;
    println!(
        "wrote {} {} tiles at levels {}-{} to {}",
        manifest.tiles.len(),
        region.id,
        region.min_level,
        region.max_level,
        options.output.display()
    );
    if options.verify {
        verify_coverage(&options.output, &region)?;
        println!("coverage verified for {}", region.id);
    }
    if let Some((latitude, longitude)) = options.probe {
        let height = probe_package(&options.output, latitude, longitude)?;
        println!("probe {latitude:.5},{longitude:.5} ellipsoid={height:.3} m");
    }
    Ok(())
}

fn verify_coverage(root: &std::path::Path, region: &RegionDefinition) -> Result<(), BakeError> {
    let manifest = ElevationPyramidManifest::from_path(root.join("manifest.ron"))?;
    let source = TileElevationSource::new(Arc::new(ZeroTerrain), manifest, 1)?;
    let bounds = region.bounds;
    let step = 0.002;
    let mut latitude = bounds.south_deg;
    while latitude <= bounds.north_deg {
        let mut longitude = bounds.west_deg;
        while longitude <= bounds.east_deg {
            let direction = terrain_lat_lon_to_body_fixed(latitude, longitude);
            if source.covering_tile_for_direction(direction).is_none() {
                return Err(BakeError::Arguments(format!(
                    "payload has no coverage at {latitude:.5},{longitude:.5}"
                )));
            }
            longitude += step;
        }
        latitude += step;
    }
    Ok(())
}

fn probe_package(root: &std::path::Path, latitude: f64, longitude: f64) -> Result<f64, BakeError> {
    let manifest = ElevationPyramidManifest::from_path(root.join("manifest.ron"))?;
    let source = TileElevationSource::new(Arc::new(ZeroTerrain), manifest, 8)?;
    let direction = terrain_lat_lon_to_body_fixed(latitude, longitude);
    let metadata = source
        .covering_tile_for_direction(direction)
        .ok_or_else(|| BakeError::Arguments("probe is outside the payload coverage".into()))?;
    let bytes = fs::read(root.join(&metadata.payload_path))?;
    source.install_tile(ElevationTile::from_bytes(&bytes)?);
    Ok(source.height_m(latitude, longitude))
}

#[derive(Debug)]
struct ZeroTerrain;

impl TerrainSource for ZeroTerrain {
    fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        ElevationBounds::new(0.0, 0.0)
    }
}

fn patches_for_region(region: &RegionDefinition) -> BTreeSet<TerrainPatch> {
    let bounds = region.bounds;
    let west = (bounds.west_deg - PATCH_MARGIN_DEG).max(-180.0);
    let east = (bounds.east_deg + PATCH_MARGIN_DEG).min(180.0);
    let south = (bounds.south_deg - PATCH_MARGIN_DEG).max(-90.0);
    let north = (bounds.north_deg + PATCH_MARGIN_DEG).min(90.0);
    let mut patches = BTreeSet::new();
    let mut latitude = south;
    while latitude <= north {
        let mut longitude = west;
        while longitude <= east {
            let direction = terrain_lat_lon_to_body_fixed(latitude, longitude);
            for level in region.min_level..=region.max_level {
                patches.insert(TerrainPatch::for_direction(direction, level));
            }
            longitude += PATCH_DISCOVERY_STEP_DEG;
        }
        latitude += PATCH_DISCOVERY_STEP_DEG;
    }
    patches
}

fn face_name(face: cosmic_systems_wasm::domain::services::cube_sphere::CubeFace) -> &'static str {
    use cosmic_systems_wasm::domain::services::cube_sphere::CubeFace;
    match face {
        CubeFace::PosX => "pos_x",
        CubeFace::NegX => "neg_x",
        CubeFace::PosY => "pos_y",
        CubeFace::NegY => "neg_y",
        CubeFace::PosZ => "pos_z",
        CubeFace::NegZ => "neg_z",
    }
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        region: DEFAULT_REGION.into(),
        config: PathBuf::from(DEFAULT_CONFIG),
        root: PathBuf::from(DEFAULT_ROOT),
        output: PathBuf::from(DEFAULT_OUTPUT),
        probe: None,
        verify: false,
    };
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--region" => {
                options.region = arguments.next().ok_or("--region requires an id")?;
            }
            "--config" => {
                options.config = PathBuf::from(arguments.next().ok_or("--config requires a path")?);
            }
            "--root" => {
                options.root = PathBuf::from(arguments.next().ok_or("--root requires a path")?);
            }
            "--output" => {
                options.output = PathBuf::from(arguments.next().ok_or("--output requires a path")?);
            }
            "--verify" => options.verify = true,
            "--probe" => {
                let latitude = arguments
                    .next()
                    .ok_or("--probe requires latitude")?
                    .parse::<f64>()
                    .map_err(|_| "--probe latitude must be a number")?;
                let longitude = arguments
                    .next()
                    .ok_or("--probe requires longitude")?
                    .parse::<f64>()
                    .map_err(|_| "--probe longitude must be a number")?;
                options.probe = Some((latitude, longitude));
            }
            "-h" | "--help" => return Err("help requested".into()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(options)
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: regional_dem_bake [--region ID] [--config PATH] [--root PATH] [--output PATH]"
    );
    ExitCode::FAILURE
}
