//! Offline equirectangular Earth imagery to cube-sphere tile converter.

use cosmic_systems_wasm::domain::services::terrain_imagery::convert_equirectangular_image;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let (Some(input), Some(output), Some(min_level), Some(max_level), Some(tile_pixels)) = (
        arguments.next(),
        arguments.next(),
        arguments.next(),
        arguments.next(),
        arguments.next(),
    ) else {
        return usage();
    };
    if arguments.next().is_some() {
        return usage();
    }
    let (Ok(min_level), Ok(max_level), Ok(tile_pixels)) = (
        min_level.to_string_lossy().parse(),
        max_level.to_string_lossy().parse(),
        tile_pixels.to_string_lossy().parse(),
    ) else {
        return usage();
    };
    match convert_equirectangular_image(
        input,
        PathBuf::from(output),
        min_level,
        max_level,
        tile_pixels,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Earth imagery conversion failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: cargo run --bin earth_imagery_convert -- <source.png> <tile-root> <min-level> <max-level> <tile-pixels>");
    ExitCode::FAILURE
}
