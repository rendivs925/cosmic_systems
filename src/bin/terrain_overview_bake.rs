//! Offline generator for body-fixed terrain overview assets.

use cosmic_systems_wasm::domain::services::terrain_overview::{
    terrain_overview_raster, TERRAIN_OVERVIEW_HEIGHT, TERRAIN_OVERVIEW_WIDTH,
};
use cosmic_systems_wasm::domain::services::terrain_source::{
    EarthTerrainSource, MarsTerrainSource, MoonTerrainSource, TerrainSource,
};
use image::RgbaImage;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(body) = arguments.next() else {
        usage_and_exit();
    };
    let Some(output_path) = arguments.next() else {
        usage_and_exit();
    };
    if arguments.next().is_some() {
        usage_and_exit();
    }

    let source: Box<dyn TerrainSource> = match body.as_str() {
        "earth" => Box::new(EarthTerrainSource::new()),
        "moon" => Box::new(
            MoonTerrainSource::new()
                .unwrap_or_else(|error| panic!("Moon terrain authority is unavailable: {error}")),
        ),
        "mars" => Box::new(
            MarsTerrainSource::new()
                .unwrap_or_else(|error| panic!("Mars terrain authority is unavailable: {error}")),
        ),
        _ => usage_and_exit(),
    };
    let pixels = terrain_overview_raster(&*source, TERRAIN_OVERVIEW_WIDTH, TERRAIN_OVERVIEW_HEIGHT);
    let image = RgbaImage::from_raw(TERRAIN_OVERVIEW_WIDTH, TERRAIN_OVERVIEW_HEIGHT, pixels)
        .expect("overview raster dimensions match its byte count");
    image
        .save(&output_path)
        .unwrap_or_else(|error| panic!("failed to write {output_path}: {error}"));
}

fn usage_and_exit() -> ! {
    eprintln!("usage: cargo run --features dem --bin terrain_overview_bake -- <earth|moon|mars> <output.png>");
    std::process::exit(2);
}
