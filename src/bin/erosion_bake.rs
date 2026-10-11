//! Offline preprocessor for Earth's deterministic erosion tiles.
//!
//! Erosion is a pure function of the base terrain, the erosion configuration,
//! and the tile key, so it can be simulated once and reused. This tool bakes
//! the *same* tiles the runtime would compute, but at a selectable quality and
//! into a persistent [`ErosionTileStore`]. The runtime then loads them instead
//! of simulating, so "ultra" erosion costs disk space rather than per-frame
//! CPU.
//!
//! The store signature matches the running simulator because both derive it
//! from [`earth_erosion_config`] and [`EARTH_EROSION_NAMESPACE`]; changing the
//! quality, seed, or DEM produces a different namespace and cannot mix tiles.
//!
//! Usage:
//!   erosion_bake --cache <dir> [--quality standard|high|ultra]
//!                [--max-bytes <n>] [--all | --region <lat_min> <lat_max> <lon_min> <lon_max>]

use cosmic_systems_wasm::domain::services::dem_terrain_source::DemTerrainSource;
use cosmic_systems_wasm::domain::services::erosion::{
    tile_bytes, ErodedTerrainSource, ErosionConfig, ErosionTileStore,
};
use cosmic_systems_wasm::domain::services::terrain_source::{
    earth_erosion_config, TerrainSource, DEFAULT_EARTH_DEM_PATH, DEFAULT_EROSION_CACHE_MAX_BYTES,
    EARTH_EROSION_NAMESPACE,
};
use std::env;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Instant;

struct Args {
    cache_dir: PathBuf,
    quality: String,
    region: (f64, f64, f64, f64),
    max_bytes: u64,
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!(
        "usage: erosion_bake --cache <dir> [--quality standard|high|ultra] \
         [--max-bytes <n>] [--all | --region <lat_min> <lat_max> <lon_min> <lon_max>]"
    );
    process::exit(2);
}

fn parse_args() -> Args {
    let mut cache_dir: Option<PathBuf> = None;
    let mut quality = "standard".to_string();
    let mut region: Option<(f64, f64, f64, f64)> = None;
    let mut max_bytes = DEFAULT_EROSION_CACHE_MAX_BYTES;
    let mut all = false;

    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--cache" => {
                let value = arguments
                    .next()
                    .unwrap_or_else(|| fail("--cache needs a value"));
                cache_dir = Some(PathBuf::from(value));
            }
            "--quality" => {
                let value = arguments
                    .next()
                    .unwrap_or_else(|| fail("--quality needs a value"));
                match value.trim().to_ascii_lowercase().as_str() {
                    "standard" | "high" | "ultra" => quality = value,
                    other => fail(&format!(
                        "unknown quality '{other}' (expected standard, high, or ultra)"
                    )),
                }
            }
            "--max-bytes" => {
                let value = arguments
                    .next()
                    .unwrap_or_else(|| fail("--max-bytes needs a value"));
                max_bytes = value
                    .parse::<u64>()
                    .ok()
                    .filter(|bytes| *bytes > 0)
                    .unwrap_or_else(|| fail("--max-bytes must be a positive integer"));
            }
            "--all" => all = true,
            "--region" => {
                let values = (0..4)
                    .map(|_| {
                        arguments
                            .next()
                            .and_then(|value| value.parse::<f64>().ok())
                            .filter(|value| value.is_finite())
                            .unwrap_or_else(|| fail("--region needs four finite numbers"))
                    })
                    .collect::<Vec<f64>>();
                region = Some((values[0], values[1], values[2], values[3]));
            }
            other => fail(&format!("unrecognized argument '{other}'")),
        }
    }

    let Some(cache_dir) = cache_dir else {
        fail("--cache <dir> is required");
    };
    let region = match (all, region) {
        (true, Some(_)) => fail("--all and --region are mutually exclusive"),
        (true, None) => (-90.0, 90.0, -180.0, 180.0),
        (false, Some(region)) => region,
        (false, None) => fail("one of --all or --region is required"),
    };
    validate_region(region);
    Args {
        cache_dir,
        quality,
        region,
        max_bytes,
    }
}

fn validate_region(region: (f64, f64, f64, f64)) {
    let (lat_min, lat_max, lon_min, lon_max) = region;
    if !(-90.0..=90.0).contains(&lat_min)
        || !(-90.0..=90.0).contains(&lat_max)
        || !(-180.0..=180.0).contains(&lon_min)
        || !(-180.0..=180.0).contains(&lon_max)
    {
        fail("--region must lie within lat [-90, 90] and lon [-180, 180]");
    }
    if lat_max <= lat_min || lon_max <= lon_min {
        fail("--region requires lat_min < lat_max and lon_min < lon_max");
    }
}

fn main() {
    let args = parse_args();

    let base: Arc<dyn TerrainSource> = Arc::new(
        DemTerrainSource::from_path(DEFAULT_EARTH_DEM_PATH)
            .unwrap_or_else(|error| panic!("Earth measured DEM is unavailable: {error}")),
    );

    // The CLI flag overrides the environment quality; the seed must match the
    // runtime so the bake signature (and therefore the cache namespace) agrees.
    let mut config = ErosionConfig::from_quality_name(&args.quality);
    config.seed = earth_erosion_config().seed;
    config.validate();

    let signature = config.bake_signature(EARTH_EROSION_NAMESPACE);
    let store = Arc::new(
        ErosionTileStore::open_with_budget(&args.cache_dir, signature, args.max_bytes)
            .unwrap_or_else(|error| panic!("cannot open erosion cache: {error}")),
    );
    let source = ErodedTerrainSource::with_store(
        Arc::clone(&base),
        config.clone(),
        Some(Arc::clone(&store)),
    );

    let tile_deg = config.tile_deg;
    let (lat_min, lat_max, lon_min, lon_max) = args.region;
    let ty_start = (lat_min / tile_deg).floor() as i64;
    let ty_end = (lat_max / tile_deg).ceil() as i64;
    let tx_start = (lon_min / tile_deg).floor() as i64;
    let tx_end = (lon_max / tile_deg).ceil() as i64;
    let total = ((ty_end - ty_start).max(0) * (tx_end - tx_start).max(0)) as usize;

    println!(
        "Baking Earth erosion: quality={} resolution={} droplets={} tiles={} \
         est_bytes={} -> {}",
        args.quality,
        config.resolution,
        config.droplets,
        total,
        total as u64 * tile_bytes(config.resolution),
        store.root().display()
    );

    let started = Instant::now();
    let mut checksum = 0.0f64;
    let mut baked = 0usize;
    let mut reused = 0usize;
    for ty in ty_start..ty_end {
        for tx in tx_start..tx_end {
            if store.contains(tx, ty) {
                reused += 1;
                continue;
            }
            let latitude_deg = (ty as f64 + 0.5) * tile_deg;
            let longitude_deg = (tx as f64 + 0.5) * tile_deg;
            checksum += source.height_m(latitude_deg, longitude_deg);
            baked += 1;
            if baked.is_multiple_of(100) {
                let elapsed = started.elapsed().as_secs_f64();
                println!(
                    "  {baked}/{} baked in {elapsed:.1}s ({:.1} tiles/s)",
                    total - reused,
                    baked as f64 / elapsed.max(f64::EPSILON)
                );
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    if let Err(error) = store.enforce_budget(args.max_bytes) {
        eprintln!("warning: could not enforce cache budget: {error}");
    }
    println!(
        "Done: baked={baked} reused={reused} total={total} in {elapsed:.1}s \
         (checksum {checksum:.3}); cache at {}",
        store.root().display()
    );
}
