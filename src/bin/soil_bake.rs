//! Offline converter: normalized soil-texture PNG -> versioned `.cssoil` package.
//!
//! Input is a north-first RGB8 raster whose channels are clay, sand, and silt
//! integer percentages (0..=100). Every sample must sum to 100. Geographic
//! bounds are terrain-radial latitude/longitude degrees, matching the runtime
//! sampler. This tool does not reproject or download data;
//! `scripts/ingest_soil.py` prepares the normalized raster and provenance.

use cosmic_systems_wasm::domain::services::soil::{SoilMetadata, SoilPackage};
use sha2::{Digest, Sha256};
use std::path::Path;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 12 {
        usage_and_exit();
    }
    let input_path = &arguments[0];
    let output_path = Path::new(&arguments[1]);
    let parse = |index: usize, name: &str| -> f64 {
        arguments[index]
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("{name} must be a number"))
    };
    let west_deg = parse(2, "west");
    let south_deg = parse(3, "south");
    let east_deg = parse(4, "east");
    let north_deg = parse(5, "north");
    let source_sha256 = arguments[6].to_ascii_lowercase();
    let source_urls = arguments[7]
        .split(';')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let source = arguments[8].clone();
    let license = arguments[9].clone();
    let attribution = arguments[10].clone();
    let source_resolution_m = parse(11, "source resolution");
    if !(west_deg < east_deg && south_deg < north_deg)
        || source_resolution_m <= 0.0
        || source_sha256.len() != 64
        || !source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || source_urls.is_empty()
    {
        usage_and_exit();
    }

    let image = image::open(input_path)
        .unwrap_or_else(|error| panic!("failed to open {input_path}: {error}"))
        .to_rgb8();
    let (width, height) = image.dimensions();
    if width < 2 || height < 2 {
        panic!("soil raster must be at least 2x2");
    }
    let mut components = Vec::with_capacity((width as usize) * (height as usize) * 3);
    let mut dominant = [0u64; 3];
    for pixel in image.pixels() {
        let [clay, sand, silt] = pixel.0;
        let sum = u32::from(clay) + u32::from(sand) + u32::from(silt);
        if sum != 100 {
            panic!("soil sample ({clay}, {sand}, {silt}) does not sum to 100");
        }
        let index = if clay >= sand && clay >= silt {
            0
        } else if sand >= silt {
            1
        } else {
            2
        };
        dominant[index] += 1;
        components.extend_from_slice(&[clay, sand, silt]);
    }

    let metadata = SoilMetadata {
        body: "Earth".into(),
        coordinate_frame: "terrain-radial-degrees".into(),
        horizontal_datum: "WGS 84 / EPSG:4326 source; planetocentric terrain-radial output grid"
            .into(),
        vertical_datum: "not applicable (surface soil texture)".into(),
        source,
        source_urls,
        source_resolution_m,
        nodata_policy: "nodata samples are replaced with neutral loam".into(),
        source_sha256,
        license,
        attribution,
        conversion_version: 1,
    };
    let package = SoilPackage::from_samples(
        width, height, west_deg, south_deg, east_deg, north_deg, metadata, components,
    )
    .unwrap_or_else(|error| panic!("invalid soil package: {error:?}"));
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
        "wrote {output_path:?}: {width}x{height}, {} bytes, sha256 {sha256}",
        bytes.len()
    );
    let total = u64::from(width) * u64::from(height);
    println!(
        "  dominant: clay {:.1}%, sand {:.1}%, silt {:.1}%",
        100.0 * dominant[0] as f64 / total as f64,
        100.0 * dominant[1] as f64 / total as f64,
        100.0 * dominant[2] as f64 / total as f64,
    );
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: cargo run --features native --bin soil_bake -- \
         <input.png> <output.cssoil> <west> <south> <east> <north> \
         <source_sha256> <source_urls;separated> <source_label> <license> \
         <attribution> <source_resolution_m>"
    );
    std::process::exit(2);
}
