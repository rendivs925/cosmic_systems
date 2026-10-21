//! Offline terrain realism metric harness. Requires `--features dem`.
//!
//! Usage:
//!   terrain_metrics [--body earth|moon|mars] [--quick|--full] [--grid WxH]
//!                   [--channel-threshold N] [--variogram-max-lag N]
//!                   [--source PATH] [--no-geoid] [--write] [--assert]
//!                   [--baseline PATH]

use cosmic_systems_wasm::domain::services::dem_terrain_source::DemTerrainSource;
use cosmic_systems_wasm::domain::services::global_raster::GlobalRasterSpec;
use cosmic_systems_wasm::domain::services::terrain_metrics::{
    compare_reports, load_report, measure_terrain_metrics, save_report, TerrainMetricsConfig,
    DEFAULT_BASELINE_PATH,
};
use cosmic_systems_wasm::domain::services::terrain_source::{
    DEFAULT_EARTH_DEM_PATH, DEFAULT_EARTH_GEOID_PATH, DEFAULT_MARS_DEM_PATH, DEFAULT_MOON_DEM_PATH,
};
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const EARTH_RADIUS_M: f64 = 6_371_008.8;
const MOON_RADIUS_M: f64 = 1_737_400.0;
const MARS_RADIUS_M: f64 = 3_389_500.0;

struct Options {
    body: String,
    source: Option<PathBuf>,
    geoid: bool,
    config: TerrainMetricsConfig,
    baseline: PathBuf,
    write: bool,
    assert: bool,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return usage();
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("terrain metrics failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let (source_path, radius_m) = match options.body.as_str() {
        "earth" => (DEFAULT_EARTH_DEM_PATH, EARTH_RADIUS_M),
        "moon" => (DEFAULT_MOON_DEM_PATH, MOON_RADIUS_M),
        "mars" => (DEFAULT_MARS_DEM_PATH, MARS_RADIUS_M),
        other => return Err(format!("unsupported body {other}").into()),
    };
    let path = options.source.unwrap_or_else(|| PathBuf::from(source_path));
    let source = if options.body == "earth" && options.geoid {
        DemTerrainSource::from_path_with_geoid(
            &path,
            DEFAULT_EARTH_GEOID_PATH,
            GlobalRasterSpec::egm2008_geoid_0p5deg(),
        )
    } else {
        DemTerrainSource::from_path(&path)
    }
    .map_err(|error| format!("failed to open {}: {error}", path.display()))?;

    let started = Instant::now();
    let report = measure_terrain_metrics(&source, &options.config, &options.body, radius_m)?;
    let elapsed = started.elapsed();

    println!(
        "{} {}x{} samples={} in {:.2}s",
        report.body,
        report.grid_width,
        report.grid_height,
        report.sample_count,
        elapsed.as_secs_f64()
    );
    println!(
        "  hypsometry min={:.1} max={:.1} land={:.4} land_mean={:.1} p50={:.1} p95={:.1}",
        report.hypsometry.min_m,
        report.hypsometry.max_m,
        report.hypsometry.land_fraction,
        report.hypsometry.land_mean_m,
        report.hypsometry.land_p50_m,
        report.hypsometry.land_p95_m
    );
    println!(
        "  slope p50={:.2} p95={:.2} max={:.2} >35deg={:.4}",
        report.slope.land_p50_deg,
        report.slope.land_p95_deg,
        report.slope.max_deg,
        report.slope.fraction_above_35_deg
    );
    println!(
        "  drainage density={:.4} km/km2 channels={} length={:.1} km strahler={}",
        report.hydrology.drainage_density_km_per_km2,
        report.hydrology.channel_cells,
        report.hydrology.channel_length_km,
        report.hydrology.max_strahler_order
    );
    println!(
        "  hack={:?} concavity={:?} sinuosity={:?}",
        report.hydrology.hack_exponent,
        report.hydrology.concavity_theta,
        report.hydrology.median_sinuosity
    );
    println!(
        "  coastline={:.1} km fractal={:?} variogram={:?} psd_beta={:?}",
        report.coastline.length_km,
        report.coastline.fractal_dimension,
        report.roughness.variogram_slope,
        report.roughness.psd_beta
    );

    if options.write {
        save_report(&options.baseline, &report)?;
        println!("wrote baseline {}", options.baseline.display());
    }
    if options.assert {
        let baseline = load_report(&options.baseline)?;
        compare_reports(&baseline, &report)?;
        println!("baseline {} matched", options.baseline.display());
    }
    Ok(())
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        body: "earth".into(),
        source: None,
        geoid: true,
        config: TerrainMetricsConfig::quick(),
        baseline: PathBuf::from(DEFAULT_BASELINE_PATH),
        write: false,
        assert: false,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--body" => {
                options.body = arguments.next().ok_or("--body requires a value")?;
            }
            "--quick" => options.config = TerrainMetricsConfig::quick(),
            "--full" => options.config = TerrainMetricsConfig::full(),
            "--grid" => {
                let value = arguments.next().ok_or("--grid requires WxH")?;
                let (width, height) = value
                    .split_once('x')
                    .ok_or("--grid expects WxH, for example 1440x720")?;
                options.config.width = width
                    .parse()
                    .map_err(|_| "--grid width must be a positive integer")?;
                options.config.height = height
                    .parse()
                    .map_err(|_| "--grid height must be a positive integer")?;
            }
            "--no-geoid" => options.geoid = false,
            "--channel-threshold" => {
                options.config.channel_threshold_cells = arguments
                    .next()
                    .ok_or("--channel-threshold requires a value")?
                    .parse()
                    .map_err(|_| "--channel-threshold must be a number")?;
            }
            "--variogram-max-lag" => {
                options.config.variogram_max_lag = arguments
                    .next()
                    .ok_or("--variogram-max-lag requires a value")?
                    .parse()
                    .map_err(|_| "--variogram-max-lag must be an integer")?;
            }
            "--source" => {
                options.source = Some(PathBuf::from(
                    arguments.next().ok_or("--source requires a path")?,
                ));
            }
            "--baseline" => {
                options.baseline =
                    PathBuf::from(arguments.next().ok_or("--baseline requires a path")?);
            }
            "--write" => options.write = true,
            "--assert" => options.assert = true,
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unrecognized argument {other}")),
        }
    }
    Ok(options)
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --features dem --bin terrain_metrics -- \
         [--body earth|moon|mars] [--quick|--full] [--grid WxH] \
         [--channel-threshold N] [--variogram-max-lag N] [--source PATH] \
         [--write] [--assert] [--baseline PATH]"
    );
    ExitCode::FAILURE
}
