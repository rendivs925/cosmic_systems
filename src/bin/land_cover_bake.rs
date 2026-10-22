//! Offline converter: normalized land-cover PNG -> versioned `.clcvr` package.
//!
//! Input is a north-first, 8-bit class raster whose values are the canonical
//! land-cover codes in `LandCoverClass` (0..=10). The geographic bounds are
//! terrain-radial latitude/longitude degrees, matching the runtime sampler.
//! This tool does not reproject or download data; `scripts/ingest_landcover.py`
//! prepares the normalized raster and provenance.

use cosmic_systems_wasm::domain::services::land_cover::{
    LandCoverClass, LandCoverMetadata, LandCoverPackage,
};
use sha2::{Digest, Sha256};
use std::path::Path;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 10 {
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
    let source = arguments[7].clone();
    let license = arguments[8].clone();
    let source_resolution_m = parse(9, "source resolution");
    if !(west_deg < east_deg && south_deg < north_deg)
        || source_resolution_m <= 0.0
        || source_sha256.len() != 64
        || !source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        usage_and_exit();
    }

    let image = image::open(input_path)
        .unwrap_or_else(|error| panic!("failed to open {input_path}: {error}"))
        .to_luma8();
    let (width, height) = image.dimensions();
    if width < 2 || height < 2 {
        panic!("land-cover raster must be at least 2x2");
    }
    let mut histogram = [0u64; 11];
    let classes = image
        .pixels()
        .map(|pixel| {
            let code = pixel.0[0];
            if code > LandCoverClass::MossLichen.code() {
                panic!(
                    "class code {code} exceeds {}",
                    LandCoverClass::MossLichen.code()
                );
            }
            histogram[code as usize] += 1;
            code
        })
        .collect::<Vec<_>>();

    let metadata = LandCoverMetadata {
        body: "Earth".into(),
        coordinate_frame: "terrain-radial-degrees".into(),
        horizontal_datum: "WGS 84 / EPSG:4326 source; terrain-radial latitude grid".into(),
        source,
        source_resolution_m,
        nodata_policy: "unknown and nodata codes fall back to bare/sparse".into(),
        source_sha256,
        license,
        conversion_version: 1,
    };
    let package = LandCoverPackage::from_samples(
        width, height, west_deg, south_deg, east_deg, north_deg, metadata, classes,
    )
    .unwrap_or_else(|error| panic!("invalid land-cover package: {error:?}"));
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
    for (code, count) in histogram.iter().enumerate() {
        if *count > 0 {
            println!(
                "  {} ({code}): {count}",
                class_name(LandCoverClass::from_code(code as u8))
            );
        }
    }
}

fn class_name(class: LandCoverClass) -> &'static str {
    match class {
        LandCoverClass::TreeCover => "tree cover",
        LandCoverClass::Shrubland => "shrubland",
        LandCoverClass::Grassland => "grassland",
        LandCoverClass::Cropland => "cropland",
        LandCoverClass::BuiltUp => "built-up",
        LandCoverClass::BareSparse => "bare/sparse",
        LandCoverClass::SnowIce => "snow/ice",
        LandCoverClass::PermanentWater => "permanent water",
        LandCoverClass::HerbaceousWetland => "herbaceous wetland",
        LandCoverClass::Mangroves => "mangroves",
        LandCoverClass::MossLichen => "moss/lichen",
    }
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: cargo run --features native --bin land_cover_bake -- \
         <input.png> <output.clcvr> <west> <south> <east> <north> \
         <source_sha256> <source_label> <license> <source_resolution_m>"
    );
    std::process::exit(2);
}
