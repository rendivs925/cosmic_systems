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
use std::time::Instant;

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
    geodetic_to_terrain_latitude_deg, RegionDefinition, RegionalDem, RegionalDemConfig,
    RegionalDemError, SourceVerticalDatum,
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
/// Acceptance gates for independent source-versus-baked sampling. They are
/// deliberately above the reviewed 30 m NASADEM resampling error but far below
/// a datum, geoid, or path regression.
const VERIFY_MAX_RMSE_M: f64 = 1.0;
const VERIFY_MAX_ABS_M: f64 = 5.0;

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
    bench_load: bool,
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
        source: if region
            .sources
            .iter()
            .all(|source| source.vertical_datum == SourceVerticalDatum::Wgs84Ellipsoid)
        {
            region.source_name.clone()
        } else {
            format!("{} (geoid-corrected)", region.source_name)
        },
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
        let error = verify_package(&options.output, &region, &dem)?;
        if error.rmse_m > VERIFY_MAX_RMSE_M || error.max_abs_m > VERIFY_MAX_ABS_M {
            return Err(BakeError::Arguments(format!(
                "source-versus-baked error exceeds acceptance: RMSE {:.3} m (max {:.1}), max {:.3} m (max {:.1})",
                error.rmse_m, VERIFY_MAX_RMSE_M, error.max_abs_m, VERIFY_MAX_ABS_M
            )));
        }
        println!(
            "coverage verified for {}; source RMSE {:.3} m, max {:.3} m over {} samples",
            region.id, error.rmse_m, error.max_abs_m, error.samples
        );
    }
    if options.bench_load {
        bench_load(&options.output, 128)?;
    }
    if let Some((latitude, longitude)) = options.probe {
        let height = probe_package(&options.output, latitude, longitude)?;
        println!("probe {latitude:.5},{longitude:.5} ellipsoid={height:.3} m");
    }
    Ok(())
}

fn bench_load(root: &std::path::Path, samples: usize) -> Result<(), BakeError> {
    let manifest_started = Instant::now();
    let manifest = ElevationPyramidManifest::from_path(root.join("manifest.ron"))?;
    let manifest_ms = manifest_started.elapsed().as_secs_f64() * 1_000.0;
    let mut durations_ms = Vec::with_capacity(samples.min(manifest.tiles.len()));
    for metadata in manifest.tiles.iter().take(samples) {
        let started = Instant::now();
        let bytes = fs::read(root.join(&metadata.payload_path))?;
        let _tile = ElevationTile::from_bytes(&bytes)?;
        durations_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    if durations_ms.is_empty() {
        return Err(BakeError::Arguments(
            "load benchmark sampled no tiles".into(),
        ));
    }
    durations_ms.sort_by(|a, b| a.total_cmp(b));
    let percentile = |percent: usize| {
        durations_ms[(durations_ms.len() * percent / 100).min(durations_ms.len() - 1)]
    };
    let p50 = percentile(50);
    let p95 = percentile(95);
    println!(
        "load benchmark: manifest {manifest_ms:.3} ms; {} tiles p50 {p50:.3} ms, p95 {p95:.3} ms",
        durations_ms.len()
    );
    if p95 > 50.0 {
        return Err(BakeError::Arguments(format!(
            "tile load p95 {p95:.3} ms exceeds the 50 ms acceptance gate"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct VerifyStats {
    samples: usize,
    rmse_m: f64,
    max_abs_m: f64,
}

fn verify_package(
    root: &std::path::Path,
    region: &RegionDefinition,
    dem: &RegionalDem,
) -> Result<VerifyStats, BakeError> {
    let manifest = ElevationPyramidManifest::from_path(root.join("manifest.ron"))?;
    let source = TileElevationSource::new(Arc::new(ZeroTerrain), manifest, 64)?;
    let bounds = region.bounds;
    let coverage_step = 0.002;
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
            longitude += coverage_step;
        }
        latitude += coverage_step;
    }

    // Compare the composed payload against the reviewed source on an
    // independent grid. The tile grid is the bake truth; this catches a bad
    // geoid, path, manifest, or resampling regression.
    let sample_step = 0.01;
    let mut samples = 0usize;
    let mut sum_squared = 0.0f64;
    let mut max_abs_m = 0.0f64;
    let mut latitude = bounds.south_deg + sample_step * 0.5;
    while latitude < bounds.north_deg {
        let mut longitude = bounds.west_deg + sample_step * 0.5;
        while longitude < bounds.east_deg {
            let direction = terrain_lat_lon_to_body_fixed(latitude, longitude);
            let metadata = source
                .covering_tile_for_direction(direction)
                .ok_or_else(|| BakeError::Arguments("payload coverage gap".into()))?;
            if !source.is_resident(metadata.patch) {
                let bytes = fs::read(root.join(&metadata.payload_path))?;
                source.install_tile(ElevationTile::from_bytes(&bytes)?);
            }
            let baked_m = source.height_m(latitude, longitude);
            let source_m = dem.height_ellipsoid_m(latitude, longitude).ok_or_else(|| {
                BakeError::Arguments(format!(
                    "source has no coverage at {latitude:.5},{longitude:.5}"
                ))
            })?;
            let error_m = baked_m - source_m;
            samples += 1;
            sum_squared += error_m * error_m;
            max_abs_m = max_abs_m.max(error_m.abs());
            longitude += sample_step;
        }
        latitude += sample_step;
    }
    if samples == 0 {
        return Err(BakeError::Arguments(
            "verification sampled no points".into(),
        ));
    }
    Ok(VerifyStats {
        samples,
        rmse_m: (sum_squared / samples as f64).sqrt(),
        max_abs_m,
    })
}

fn probe_package(root: &std::path::Path, latitude: f64, longitude: f64) -> Result<f64, BakeError> {
    let manifest = ElevationPyramidManifest::from_path(root.join("manifest.ron"))?;
    let source = TileElevationSource::new(Arc::new(ZeroTerrain), manifest, 8)?;
    let terrain_latitude = geodetic_to_terrain_latitude_deg(latitude);
    let direction = terrain_lat_lon_to_body_fixed(terrain_latitude, longitude);
    let metadata = source
        .covering_tile_for_direction(direction)
        .ok_or_else(|| BakeError::Arguments("probe is outside the payload coverage".into()))?;
    let bytes = fs::read(root.join(&metadata.payload_path))?;
    source.install_tile(ElevationTile::from_bytes(&bytes)?);
    Ok(source.height_m(terrain_latitude, longitude))
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
        bench_load: false,
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
            "--bench-load" => options.bench_load = true,
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
        "usage: regional_dem_bake [--region ID] [--config PATH] [--root PATH] [--output PATH] [--verify] [--bench-load] [--probe LAT LON]"
    );
    ExitCode::FAILURE
}
