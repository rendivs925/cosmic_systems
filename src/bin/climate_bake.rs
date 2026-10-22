//! Offline converter: normalized climate raster -> versioned `.csclim` package.
//!
//! Input is an interleaved little-endian float32 raster laid out as
//! `[month][row][column][channel]`, north-first, with terrain-radial bounds.
//! Channels are temperature degC, pressure hPa, relative humidity percent,
//! eastward wind m/s, northward wind m/s, and cloud amount percent.
//! `scripts/ingest_climate.py` prepares the raster and provenance; this tool
//! never downloads or reprojects data.

use cosmic_systems_wasm::domain::services::climate::{
    quantize, ClimateMetadata, ClimatePackage, ClimateSample,
};
use sha2::{Digest, Sha256};
use std::path::Path;

const CHANNELS: usize = 6;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 16 {
        usage_and_exit();
    }
    let input_path = &arguments[0];
    let output_path = Path::new(&arguments[1]);
    let parse_f64 = |index: usize, name: &str| -> f64 {
        arguments[index]
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("{name} must be a number"))
    };
    let width = arguments[2]
        .parse::<u32>()
        .unwrap_or_else(|_| panic!("width must be an integer"));
    let height = arguments[3]
        .parse::<u32>()
        .unwrap_or_else(|_| panic!("height must be an integer"));
    let months = arguments[4]
        .parse::<u32>()
        .unwrap_or_else(|_| panic!("months must be an integer"));
    let west_deg = parse_f64(5, "west");
    let south_deg = parse_f64(6, "south");
    let east_deg = parse_f64(7, "east");
    let north_deg = parse_f64(8, "north");
    let source_sha256 = arguments[9].to_ascii_lowercase();
    let source_urls = arguments[10]
        .split(';')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let source = arguments[11].clone();
    let license = arguments[12].clone();
    let attribution = arguments[13].clone();
    let source_resolution_deg = parse_f64(14, "source resolution");
    let reference_period = arguments[15].clone();
    if !(west_deg < east_deg && south_deg < north_deg)
        || width < 2
        || height < 2
        || !(1..=12).contains(&months)
        || source_resolution_deg <= 0.0
        || source_sha256.len() != 64
        || !source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || source_urls.is_empty()
    {
        usage_and_exit();
    }

    let raw = std::fs::read(input_path)
        .unwrap_or_else(|error| panic!("failed to read {input_path}: {error}"));
    let expected_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|count| count.checked_mul(months as usize))
        .and_then(|count| count.checked_mul(CHANNELS))
        .and_then(|count| count.checked_mul(4))
        .unwrap_or_else(|| panic!("climate raster dimensions overflow"));
    if raw.len() != expected_bytes {
        panic!(
            "climate raster has {} bytes, expected {expected_bytes} for {width}x{height}x{months}",
            raw.len()
        );
    }

    let mut cells = Vec::with_capacity(expected_bytes / 4 * 2);
    let mut ranges = [[f32::INFINITY, f32::NEG_INFINITY]; CHANNELS];
    for chunk in raw.chunks_exact(4 * CHANNELS) {
        let mut values = [0.0f32; CHANNELS];
        for (channel, value) in values.iter_mut().enumerate() {
            let offset = channel * 4;
            *value = f32::from_le_bytes(chunk[offset..offset + 4].try_into().unwrap());
            if !value.is_finite() {
                panic!("climate raster contains a non-finite value");
            }
            ranges[channel][0] = ranges[channel][0].min(*value);
            ranges[channel][1] = ranges[channel][1].max(*value);
        }
        let sample = ClimateSample {
            temperature_k: f64::from(values[0]) + 273.15,
            pressure_pa: f64::from(values[1]) * 100.0,
            relative_humidity: f64::from(values[2]) / 100.0,
            wind_east_mps: f64::from(values[3]),
            wind_north_mps: f64::from(values[4]),
            cloud_fraction: f64::from(values[5]) / 100.0,
        };
        quantize(&mut cells, sample);
    }

    let metadata = ClimateMetadata {
        body: "Earth".into(),
        coordinate_frame: "terrain-radial-degrees".into(),
        horizontal_datum: "WGS 84 / EPSG:4326 source; planetocentric terrain-radial output grid"
            .into(),
        vertical_datum: "not applicable (surface climatology)".into(),
        source,
        source_urls,
        source_resolution_deg,
        reference_period,
        nodata_policy: "all reviewed source cells are finite; no nodata remains".into(),
        source_sha256,
        license,
        attribution,
        conversion_version: 1,
    };
    let package = ClimatePackage::from_cells(
        width, height, months, west_deg, south_deg, east_deg, north_deg, metadata, cells,
    )
    .unwrap_or_else(|error| panic!("invalid climate package: {error:?}"));
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("failed to create {parent:?}: {error}"));
    }
    package
        .write_path(output_path)
        .unwrap_or_else(|error| panic!("failed to write {output_path:?}: {error:?}"));
    let bytes = std::fs::read(output_path).expect("written package must be readable");
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!(
        "wrote {output_path:?}: {width}x{height}x{months}, {} bytes, sha256 {sha256}",
        bytes.len()
    );
    let labels = [
        "temperature degC",
        "pressure hPa",
        "relative humidity %",
        "wind east m/s",
        "wind north m/s",
        "cloud amount %",
    ];
    for (label, range) in labels.iter().zip(ranges) {
        println!("  {label}: {:.2}..{:.2}", range[0], range[1]);
    }
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: cargo run --features native --bin climate_bake -- \
         <input.raw> <output.csclim> <width> <height> <months> <west> <south> \
         <east> <north> <source_sha256> <source_urls;separated> <source_label> \
         <license> <attribution> <source_resolution_deg> <reference_period>"
    );
    std::process::exit(2);
}
