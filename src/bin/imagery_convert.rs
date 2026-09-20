//! Offline converter: equirectangular Earth imagery -> cube-sphere tiles.
//!
//! Input is an already normalized equirectangular RGB(A) image in WGS 84
//! body-fixed coordinates (the imagery equivalent of the normalized DEM that
//! `local_elevation_convert` accepts). This tool does not reproject UTM or
//! infer transforms; a Sentinel-2 granule must be reprojected to WGS 84
//! geographic before conversion.
//!
//! Output layout matches the imagery manifest:
//! `<output_dir>/<face>/<level>/<tile_x>_<tile_y>.png`

use cosmic_systems_wasm::domain::services::imagery_tiles::{
    cube_face_name, render_imagery_tile, tiles_for_region, EquirectangularRgb,
};
use cosmic_systems_wasm::domain::value_objects::imagery_manifest::GeoBounds;
use image::RgbaImage;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 9 {
        usage_and_exit();
    }
    let input_path = &arguments[0];
    let output_dir = std::path::Path::new(&arguments[1]);
    let parse = |index: usize, name: &str| -> f64 {
        arguments[index]
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("{name} must be a number"))
    };
    let bounds = GeoBounds {
        west_deg: parse(2, "west"),
        south_deg: parse(3, "south"),
        east_deg: parse(4, "east"),
        north_deg: parse(5, "north"),
    };
    let min_level: u32 = arguments[6].parse().unwrap_or_else(|_| usage_and_exit());
    let max_level: u32 = arguments[7].parse().unwrap_or_else(|_| usage_and_exit());
    let resolution: u32 = arguments[8].parse().unwrap_or_else(|_| usage_and_exit());
    if !(bounds.west_deg < bounds.east_deg && bounds.south_deg < bounds.north_deg)
        || min_level > max_level
        || resolution < 2
    {
        usage_and_exit();
    }

    let source_image = image::open(input_path)
        .unwrap_or_else(|error| panic!("failed to open {input_path}: {error}"))
        .to_rgba8();
    let (width, height) = source_image.dimensions();
    let pixels = source_image
        .pixels()
        .map(|pixel| pixel.0)
        .collect::<Vec<[u8; 4]>>();
    let source = EquirectangularRgb::new(width, height, pixels)
        .unwrap_or_else(|error| panic!("invalid source image: {error:?}"));

    let mut written = 0usize;
    for level in min_level..=max_level {
        let tiles = tiles_for_region(bounds, level);
        for patch in tiles {
            let raster = render_imagery_tile(&source, &patch, resolution);
            let image = RgbaImage::from_raw(resolution, resolution, flatten(raster))
                .expect("tile dimensions match its byte count");
            let directory = output_dir
                .join(cube_face_name(patch.face))
                .join(level.to_string());
            std::fs::create_dir_all(&directory)
                .unwrap_or_else(|error| panic!("failed to create {directory:?}: {error}"));
            let path = directory.join(format!("{}_{}.png", patch.tile_x, patch.tile_y));
            image
                .save(&path)
                .unwrap_or_else(|error| panic!("failed to write {path:?}: {error}"));
            written += 1;
        }
    }
    println!("wrote {written} imagery tiles at levels {min_level}..={max_level} to {output_dir:?}");
}

fn flatten(pixels: Vec<[u8; 4]>) -> Vec<u8> {
    pixels.into_iter().flatten().collect()
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: cargo run --features native --bin imagery_convert -- \
         <input.png> <output_dir> <west> <south> <east> <north> <min_level> <max_level> <resolution>"
    );
    std::process::exit(2);
}
