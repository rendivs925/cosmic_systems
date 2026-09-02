//! Offline ETOPO1 raw-raster converter. Requires `--features dem`.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use cosmic_systems_wasm::domain::services::dem_terrain_source::convert_etopo1_raw;

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let Some(input) = arguments.next() else {
        return usage();
    };
    let Some(output) = arguments.next() else {
        return usage();
    };
    let resolution = match arguments.next() {
        Some(value) => match value.to_string_lossy().parse::<u32>() {
            Ok(value) => value,
            Err(_) => return usage(),
        },
        None => 2_048,
    };
    if arguments.next().is_some() {
        return usage();
    }

    match convert_etopo1_raw(PathBuf::from(input), PathBuf::from(output), resolution) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ETOPO1 conversion failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: cargo run --features dem --bin etopo1_convert -- <input.raw> <output.csdem> [face-resolution]");
    ExitCode::FAILURE
}
