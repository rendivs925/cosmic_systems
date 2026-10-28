//! Offline converter: normalized water/tree-cover/canopy-height PNGs ->
//! `.cssfld` package.
//!
//! Inputs are north-first 8-bit rasters. Water occurrence and tree cover are
//! percentages (`0..=100`); canopy height is whole metres (`0..=255`), with zero
//! meaning no measurement. The geographic bounds are terrain-radial
//! latitude/longitude degrees, matching the runtime sampler. This tool does not
//! reproject or download data; `scripts/ingest_water_canopy.py` prepares the
//! normalized rasters.

use cosmic_systems_wasm::domain::services::surface_fields::{
    SurfaceFieldsMetadata, SurfaceFieldsPackage, SURFACE_FIELD_NAMES,
};
use sha2::{Digest, Sha256};
use std::path::Path;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 16 {
        usage_and_exit();
    }
    let water_path = &arguments[0];
    let tree_path = &arguments[1];
    let canopy_path = &arguments[2];
    let output_path = Path::new(&arguments[3]);
    let parse = |index: usize, name: &str| -> f64 {
        arguments[index]
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("{name} must be a number"))
    };
    let west_deg = parse(4, "west");
    let south_deg = parse(5, "south");
    let east_deg = parse(6, "east");
    let north_deg = parse(7, "north");
    let source_sha256 = arguments[8].to_ascii_lowercase();
    let source_urls = vec![
        arguments[9].clone(),
        arguments[10].clone(),
        arguments[11].clone(),
    ];
    let source = arguments[12].clone();
    let license = arguments[13].clone();
    let attribution = arguments[14].clone();
    let source_resolution_m = parse(15, "source resolution");
    if !(west_deg < east_deg && south_deg < north_deg)
        || source_resolution_m <= 0.0
        || source_sha256.len() != 64
        || !source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        usage_and_exit();
    }

    let water = image::open(water_path)
        .unwrap_or_else(|error| panic!("failed to open {water_path}: {error}"))
        .to_luma8();
    let tree = image::open(tree_path)
        .unwrap_or_else(|error| panic!("failed to open {tree_path}: {error}"))
        .to_luma8();
    let canopy = image::open(canopy_path)
        .unwrap_or_else(|error| panic!("failed to open {canopy_path}: {error}"))
        .to_luma8();
    let (width, height) = water.dimensions();
    if width < 2
        || height < 2
        || tree.dimensions() != (width, height)
        || canopy.dimensions() != (width, height)
    {
        panic!("water, tree-cover, and canopy-height rasters must share a 2x2-or-larger grid");
    }
    let mut values = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for ((water, tree), canopy) in water.pixels().zip(tree.pixels()).zip(canopy.pixels()) {
        let [water] = water.0;
        let [tree] = tree.0;
        let [canopy] = canopy.0;
        if water > 100 || tree > 100 {
            panic!("surface-field sample ({water}, {tree}) exceeds 100 percent");
        }
        values.extend_from_slice(&[water, tree, canopy]);
    }

    let metadata = SurfaceFieldsMetadata {
        body: "Earth".into(),
        coordinate_frame: "terrain-radial-degrees".into(),
        horizontal_datum: "WGS 84 / EPSG:4326 source; planetocentric terrain-radial output grid"
            .into(),
        vertical_datum: "not applicable (surface water, canopy cover, canopy height)".into(),
        source,
        source_urls,
        field_names: SURFACE_FIELD_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect(),
        source_resolution_m,
        nodata_policy:
            "nodata falls back to zero water occurrence, zero tree cover, and zero measured canopy height"
                .into(),
        source_sha256,
        license,
        attribution,
        conversion_version: 2,
    };
    let package = SurfaceFieldsPackage::from_samples(
        width, height, west_deg, south_deg, east_deg, north_deg, metadata, values,
    )
    .unwrap_or_else(|error| panic!("invalid surface-field package: {error:?}"));
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
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: cargo run --features native --bin surface_fields_bake -- \
         <water.png> <tree_cover.png> <canopy_height.png> <output.cssfld> \
         <west> <south> <east> <north> <source_sha256> <water_url> <tree_url> \
         <canopy_url> <source_label> <license> <attribution> <source_resolution_m>"
    );
    std::process::exit(2);
}
