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
    tile_bytes, BaseProbe, ErodedTerrainSource, ErosionConfig, ErosionTileStore,
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

fn usage() {
    eprintln!(
        "usage: erosion_bake --cache <dir> [--quality standard|high|ultra] \
         [--max-bytes <n>] [--all | --region <lat_min> <lat_max> <lon_min> <lon_max>]"
    );
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    usage();
    process::exit(2);
}

fn parse_args() -> Args {
    match parse_args_from(env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => fail(&message),
    }
}

/// Parse and validate arguments from an explicit iterator so the CLI contract is
/// testable without spawning a process or exiting.
fn parse_args_from<I: Iterator<Item = String>>(mut arguments: I) -> Result<Args, String> {
    let mut cache_dir: Option<PathBuf> = None;
    let mut quality = "standard".to_string();
    let mut region: Option<(f64, f64, f64, f64)> = None;
    let mut max_bytes = DEFAULT_EROSION_CACHE_MAX_BYTES;
    let mut all = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--cache" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--cache needs a value".to_string())?;
                cache_dir = Some(PathBuf::from(value));
            }
            "--quality" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--quality needs a value".to_string())?;
                match value.trim().to_ascii_lowercase().as_str() {
                    "standard" | "high" | "ultra" => quality = value,
                    other => {
                        return Err(format!(
                            "unknown quality '{other}' (expected standard, high, or ultra)"
                        ))
                    }
                }
            }
            "--max-bytes" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-bytes needs a value".to_string())?;
                max_bytes = value
                    .parse::<u64>()
                    .ok()
                    .filter(|bytes| *bytes > 0)
                    .ok_or_else(|| "--max-bytes must be a positive integer".to_string())?;
            }
            "--all" => all = true,
            "--region" => {
                let mut values = [0.0f64; 4];
                for slot in &mut values {
                    let value = arguments
                        .next()
                        .ok_or_else(|| "--region needs four finite numbers".to_string())?;
                    *slot = value
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| "--region needs four finite numbers".to_string())?;
                }
                region = Some((values[0], values[1], values[2], values[3]));
            }
            other => return Err(format!("unrecognized argument '{other}'")),
        }
    }

    let cache_dir = cache_dir.ok_or_else(|| "--cache <dir> is required".to_string())?;
    let region = match (all, region) {
        (true, Some(_)) => return Err("--all and --region are mutually exclusive".to_string()),
        (true, None) => (-90.0, 90.0, -180.0, 180.0),
        (false, Some(region)) => region,
        (false, None) => return Err("one of --all or --region is required".to_string()),
    };
    validate_region(region)?;
    Ok(Args {
        cache_dir,
        quality,
        region,
        max_bytes,
    })
}

fn validate_region(region: (f64, f64, f64, f64)) -> Result<(), String> {
    let (lat_min, lat_max, lon_min, lon_max) = region;
    if !(-90.0..=90.0).contains(&lat_min)
        || !(-90.0..=90.0).contains(&lat_max)
        || !(-180.0..=180.0).contains(&lon_min)
        || !(-180.0..=180.0).contains(&lon_max)
    {
        return Err("--region must lie within lat [-90, 90] and lon [-180, 180]".to_string());
    }
    if lat_max <= lat_min || lon_max <= lon_min {
        return Err("--region requires lat_min < lat_max and lon_min < lon_max".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(list: &[&str]) -> Result<Args, String> {
        parse_args_from(list.iter().map(|value| value.to_string()))
    }

    #[test]
    fn defaults_to_standard_quality_and_the_all_region() {
        let parsed = parse(&["--cache", "/tmp/x", "--all"]).expect("parse");
        assert_eq!(parsed.quality, "standard");
        assert_eq!(parsed.region, (-90.0, 90.0, -180.0, 180.0));
        assert_eq!(parsed.max_bytes, DEFAULT_EROSION_CACHE_MAX_BYTES);
        assert_eq!(parsed.cache_dir, PathBuf::from("/tmp/x"));
    }

    #[test]
    fn parses_quality_region_and_budget() {
        let parsed = parse(&[
            "--cache",
            "/c",
            "--quality",
            "ultra",
            "--max-bytes",
            "2048",
            "--region",
            "10",
            "12",
            "20",
            "22",
        ])
        .expect("parse");
        assert_eq!(parsed.quality, "ultra");
        assert_eq!(parsed.region, (10.0, 12.0, 20.0, 22.0));
        assert_eq!(parsed.max_bytes, 2048);
    }

    #[test]
    fn rejects_missing_or_conflicting_selectors() {
        assert!(parse(&["--all"]).is_err());
        assert!(parse(&["--cache", "/c"]).is_err());
        assert!(parse(&["--cache", "/c", "--all", "--region", "0", "1", "0", "1"]).is_err());
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(parse(&["--cache", "/c", "--quality", "bogus", "--all"]).is_err());
        assert!(parse(&["--cache", "/c", "--max-bytes", "0", "--all"]).is_err());
        assert!(parse(&["--cache", "/c", "--region", "0", "1", "0"]).is_err());
        assert!(parse(&["--cache", "/c", "--region", "5", "5", "0", "1"]).is_err());
        assert!(parse(&["--cache", "/c", "--region", "-95", "1", "0", "1"]).is_err());
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
            // Reuse only a tile that still decodes *and* whose recorded base
            // probe matches the live DEM. A corrupt, stale, or foreign tile is
            // re-baked rather than counted as reused.
            let lat_min = ty as f64 * tile_deg;
            let lon_min = tx as f64 * tile_deg;
            let live_probe = BaseProbe::sample(
                base.as_ref(),
                lat_min,
                lat_min + tile_deg,
                lon_min,
                lon_min + tile_deg,
            );
            let cached = store
                .load(tx, ty, config.resolution, tile_deg)
                .is_some_and(|(_, stored_probe)| stored_probe.matches(live_probe));
            if cached {
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
    let write_failures = store.write_failures();
    if write_failures > 0 {
        eprintln!("warning: {write_failures} tile(s) could not be persisted");
    }
    println!(
        "Done: baked={baked} reused={reused} total={total} in {elapsed:.1}s \
         (checksum {checksum:.3}, write_failures={write_failures}); cache at {}",
        store.root().display()
    );
}
